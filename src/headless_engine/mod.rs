mod addons;
mod auth;
mod calendar;
mod contracts;
mod detail;
mod discover;
mod helpers;
mod home;
mod library;
mod navigation;
mod offline;
mod player;
mod plugins;
mod profile;
mod search;
mod settings;
mod state;
mod sync;
mod trailer;
mod youtube_cipher;

use crate::core_error::{CoreError, LogAndDiscard};
use crate::runtime::{EffectEnvelope, EffectKind};
use contracts::{AppAction, DispatchResult, StatePatch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use state::{EngineState, GenerationKey};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use web_time::Instant;

pub(crate) use contracts::EffectResultInput;

// If the platform never calls complete_effect for an effect (a transient IPC failure on
// the completion call, a swallowed exception, etc.), it would otherwise sit in
// pending_effects/delivered_effect_ids forever for the life of the engine instance.
// Anything genuinely still in flight completes well within this window.
const EFFECT_EXPIRY: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeadlessEngine {
    #[serde(default)]
    state: EngineState,
    #[serde(default = "first_effect_id")]
    next_effect_id: u64,
    // Ids handed to the platform at least once, awaiting their complete_effect call.
    // Never serialized — purely tracks delivery so the "drain the queue" fallback in
    // resolve_visible_effects doesn't hand out an effect that's already in flight as
    // if it were fresh work (which used to make an unrelated dispatch while a slow
    // effect was still running re-trigger a full duplicate execution of it).
    #[serde(skip)]
    delivered_effect_ids: HashSet<String>,
    // When each pending effect was created, for expire_stale_pending_effects. Never
    // serialized — Instant isn't a portable wall-clock value, just an internal timer.
    #[serde(skip)]
    effect_created_at: HashMap<String, Instant>,
}

fn first_effect_id() -> u64 {
    1
}

static ENGINE_COUNTER: AtomicU64 = AtomicU64::new(1);
static ENGINES: OnceLock<Mutex<HashMap<u64, HeadlessEngine>>> = OnceLock::new();

pub fn create_headless_engine(initial_json: &str) -> u64 {
    let mut engine = HeadlessEngine {
        next_effect_id: 1,
        ..HeadlessEngine::default()
    };
    if let Ok(initial_state) = serde_json::from_str::<EngineState>(initial_json) {
        engine.state = initial_state;
    }
    let mut map = lock_engines();
    let handle = ENGINE_COUNTER.fetch_add(1, Ordering::Relaxed);
    map.insert(handle, engine);
    handle
}

pub fn destroy_headless_engine(handle: u64) -> bool {
    lock_engines().remove(&handle).is_some()
}

pub fn headless_engine_snapshot_json(handle: u64) -> Option<String> {
    let state = {
        let map = lock_engines();
        map.get(&handle)?.state.clone()
    };
    serde_json::to_string(&state).ok()
}

pub fn headless_engine_dispatch_json(handle: u64, action_json: &str) -> Option<String> {
    let action: AppAction = serde_json::from_str(action_json)
        .map_err(|e| CoreError::BadInput {
            context: "headless_engine_dispatch_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let (patch, visible_effects) = {
        let mut map = lock_engines();
        let engine = match map.get_mut(&handle) {
            Some(engine) => engine,
            None => {
                return CoreError::NotFound {
                    context: "headless_engine_dispatch_json",
                }
                .log_and_none();
            }
        };
        engine.expire_stale_pending_effects(Instant::now());
        let effects = engine.dispatch(action);
        let visible_effects = engine.resolve_visible_effects(effects);
        (engine.state.diff_dirty(), visible_effects)
    };
    result_patch_json(patch, visible_effects)
}

pub fn headless_engine_complete_effect_json(handle: u64, result_json: &str) -> Option<String> {
    let result: EffectResultInput = serde_json::from_str(result_json)
        .map_err(|e| CoreError::BadInput {
            context: "headless_engine_complete_effect_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let (patch, visible_effects) = {
        let mut map = lock_engines();
        let engine = match map.get_mut(&handle) {
            Some(engine) => engine,
            None => {
                return CoreError::NotFound {
                    context: "headless_engine_complete_effect_json",
                }
                .log_and_none();
            }
        };
        engine.expire_stale_pending_effects(Instant::now());
        let effects = engine.complete_effect(result);
        let visible_effects = engine.resolve_visible_effects(effects);
        (engine.state.diff_dirty(), visible_effects)
    };
    result_patch_json(patch, visible_effects)
}

impl HeadlessEngine {
    fn dispatch(&mut self, action: AppAction) -> Vec<EffectEnvelope> {
        match action {
            AppAction::NavigationRequested { route, params } => {
                navigation::dispatch(self, route, params)
            }
            AppAction::DetailLoadRequested {
                content_type,
                id,
                language,
                source_addon_transport_url,
                source_addon_catalog_type,
                profile,
            } => detail::dispatch_load(
                self,
                content_type,
                id,
                language,
                source_addon_transport_url,
                source_addon_catalog_type,
                profile,
            ),
            AppAction::DetailLocalStateRequested {
                primary_id,
                fallback_id,
                content_type,
                profile,
            } => detail::dispatch_local_state(self, primary_id, fallback_id, content_type, profile),
            AppAction::DetailSecondaryRequested {
                content_type,
                id,
                language,
                profile,
                similar_titles_source,
            } => detail::dispatch_secondary(
                self,
                content_type,
                id,
                language,
                profile,
                similar_titles_source,
            ),
            AppAction::DetailPrefetchRequested {
                content_type,
                id,
                stream_lookup_id,
                title,
                original_name,
                year,
                language,
                profile,
            } => detail::dispatch_prefetch(
                self,
                content_type,
                id,
                stream_lookup_id,
                title,
                original_name,
                year,
                language,
                profile,
            ),
            AppAction::DetailStreamsRequested {
                content_type,
                request_ids,
                detail,
                season_episodes,
                language,
                profile,
            } => detail::dispatch_streams(
                self,
                content_type,
                request_ids,
                detail,
                season_episodes,
                language,
                profile,
            ),
            AppAction::DetailStreamsAppended {
                streams,
                available_addons,
                generation,
            } => detail::dispatch_streams_appended(self, streams, available_addons, generation),
            AppAction::DetailSelectedAddonChanged { addon } => {
                detail::dispatch_selected_addon_changed(self, addon)
            }
            AppAction::MetaDetailRequested {
                content_type,
                id,
                language,
                profile,
            } => detail::dispatch_meta_detail(self, content_type, id, language, profile),
            AppAction::DirectPlaybackRequested {
                meta,
                language,
                profile,
            } => home::dispatch_direct_playback(self, meta, language, profile),
            AppAction::IntroSegmentsRequested {
                imdb_id,
                season,
                episode,
                title,
                use_intro_db,
                use_ani_skip,
            } => player::dispatch_intro_segments(
                self,
                imdb_id,
                season,
                episode,
                title,
                use_intro_db,
                use_ani_skip,
            ),
            AppAction::IntroImdbIdRequested {
                meta,
                video_id,
                language,
            } => player::dispatch_intro_imdb_id(self, meta, video_id, language),
            AppAction::PlayerLoadStreamsRequested {
                content_type,
                id,
                current_video_id,
                initial_video_id,
                initial_streams,
                initial_stream_index,
                saved_url,
                saved_title,
                source_selection_mode,
                regex_pattern,
                preferred_binge_group,
                title,
                original_name,
                year,
                language,
                profile,
                outgoing_progress,
            } => player::dispatch_load_streams(
                self,
                content_type,
                id,
                current_video_id,
                initial_video_id,
                initial_streams,
                initial_stream_index,
                saved_url,
                saved_title,
                source_selection_mode,
                regex_pattern,
                preferred_binge_group,
                title,
                original_name,
                year,
                language,
                profile,
                outgoing_progress,
            ),
            AppAction::PlayerStreamsLoaded {
                streams,
                current_video_id,
                initial_stream_index,
                saved_url,
                saved_title,
                source_selection_mode,
                regex_pattern,
                preferred_binge_group,
            } => player::dispatch_streams_loaded(
                self,
                streams,
                current_video_id,
                initial_stream_index,
                saved_url,
                saved_title,
                source_selection_mode,
                regex_pattern,
                preferred_binge_group,
            ),
            AppAction::PlayerStreamsFailed { error_code } => {
                player::dispatch_streams_failed(self, error_code)
            }
            AppAction::PlayerResolvePlaybackRequested {
                url,
                stream,
                current_video_id,
                title,
            } => player::dispatch_resolve_playback(self, url, stream, current_video_id, title),
            AppAction::ScrobbleRequested {
                token,
                meta_type,
                item_id,
                progress,
                action_name,
                profile,
            } => player::dispatch_scrobble(
                self,
                token,
                meta_type,
                item_id,
                progress,
                action_name,
                profile,
            ),
            AppAction::ProfileActivated { profile } => {
                library::dispatch_profile_activated(self, profile)
            }
            AppAction::HomeLoadRequested {
                profile,
                language,
                force,
            } => home::dispatch_load(self, profile, language, force),
            AppAction::RefreshContinueWatchingRequested { profile, language } => {
                home::dispatch_refresh_continue_watching(self, profile, language)
            }
            AppAction::LibraryHydrateRequested { profile_id } => {
                library::dispatch_hydrate(self, profile_id)
            }
            AppAction::ToggleWatchlistRequested { item, profile } => {
                library::dispatch_toggle_watchlist(self, item, profile)
            }
            AppAction::ToggleLibraryStatusRequested { list, item } => {
                library::dispatch_toggle_status(self, list, item)
            }
            AppAction::SetFeedbackRequested { id, value, meta } => {
                library::dispatch_set_feedback(self, id, value, meta)
            }
            AppAction::ClearPlaybackProgressRequested { profile, meta } => {
                library::dispatch_clear_progress(self, profile, meta)
            }
            AppAction::SavePlaybackProgressRequested { action } => library::dispatch_save_progress(
                self,
                action.profile,
                action.meta,
                action.time_offset,
                action.duration,
                action.last_video_id,
                action.last_stream_index,
                action.last_episode_name,
                action.last_episode_season,
                action.last_episode_number,
                action.last_episode_thumbnail,
                action.last_stream_url,
                action.last_stream_title,
                action.last_audio_language,
                action.last_subtitle_language,
                action.scrobble_trakt_pause,
            ),
            AppAction::MarkWatchedRequested { action } => library::dispatch_mark_watched(
                self,
                action.series_id,
                action.video_ids,
                action.watched,
                action.meta,
                action.episodes,
                action.profile,
            ),
            AppAction::AddonInstallRequested {
                transport_url,
                force_refresh,
            } => addons::dispatch_install(self, transport_url, force_refresh),
            AppAction::AddonsRefreshRequested {
                profile,
                force_refresh,
            } => addons::dispatch_refresh(self, profile, force_refresh),
            AppAction::AddonResourceRequested {
                transport_url,
                resource,
                content_type,
                id,
                extra,
            } => addons::dispatch_resource(self, transport_url, resource, content_type, id, extra),
            AppAction::SearchRequested {
                query,
                profile,
                language,
            } => search::dispatch(self, query, profile, language),
            AppAction::DiscoverRequested {
                content_type,
                filters,
                profile,
                language,
            } => discover::dispatch_discover(self, content_type, filters, profile, language),
            AppAction::DiscoverCatalogFiltersRequested {
                content_type,
                selected_catalog_key,
                profile,
                language,
            } => discover::dispatch_catalog_filters(
                self,
                content_type,
                selected_catalog_key,
                profile,
                language,
            ),
            AppAction::DiscoverPageRequested {
                transport_url,
                content_type,
                catalog_id,
                skip,
                genre,
            } => discover::dispatch_discover_page(
                self,
                transport_url,
                content_type,
                catalog_id,
                skip,
                genre,
            ),
            AppAction::CatalogPageRequested {
                category_id,
                transport_url,
                content_type,
                catalog_id,
                skip,
                genre,
                search,
                remote_source,
                profile,
            } => home::dispatch_catalog_page(
                self,
                category_id,
                transport_url,
                content_type,
                catalog_id,
                skip,
                genre,
                search,
                remote_source,
                profile,
            ),
            AppAction::DetailSeasonRequested {
                series_id,
                season,
                profile,
                language,
            } => detail::dispatch_season(self, series_id, season, profile, language),
            AppAction::PlayerNextEpisodeCardShown {
                content_type,
                series_id,
                next_video_id,
                title,
                original_name,
                year,
                language,
                profile,
            } => player::dispatch_next_episode_prefetch(
                self,
                content_type,
                series_id,
                next_video_id,
                title,
                original_name,
                year,
                language,
                profile,
            ),
            AppAction::SubtitleLoadRequested {
                stream,
                content_type,
                id,
                extra_args,
            } => player::dispatch_subtitle_load(self, stream, content_type, id, extra_args),
            AppAction::ExternalSyncRequested {
                provider,
                profile,
                language,
            } => sync::dispatch_external_sync(self, provider, profile, language),
            AppAction::AuthFlowRequested { provider, mode } => {
                auth::dispatch_flow(self, provider, mode)
            }
            AppAction::AuthExchangeRequested {
                provider,
                code,
                code_verifier,
                profile,
            } => auth::dispatch_exchange(self, provider, code, code_verifier, profile),
            AppAction::AuthRefreshRequested { provider, profile } => {
                auth::dispatch_token_refresh(self, provider, profile)
            }
            AppAction::ExternalIntegrationSyncRequested {
                provider,
                profile,
                language,
            } => sync::dispatch_integration_sync(self, provider, profile, language),
            AppAction::SettingsChanged { key, value } => settings::dispatch(self, key, value),
            AppAction::CalendarMonthRequested {
                profile,
                year,
                month,
                planned_items,
            } => calendar::dispatch(self, profile, year, month, planned_items),
            AppAction::OfflineDownloadRequested {
                meta,
                stream,
                video_id,
                video,
                subtitle,
                profile_id,
                language,
            } => offline::dispatch(
                self, meta, stream, video_id, video, subtitle, profile_id, language,
            ),
            AppAction::TrailerResolveRequested {
                request_id,
                video_id,
                max_height,
            } => trailer::dispatch_resolve(self, request_id, video_id, max_height),
            AppAction::TrailerPrewarmRequested => trailer::dispatch_prewarm(self),
            AppAction::PluginRepositoryAddRequested { manifest_url } => {
                plugins::dispatch_add_repository(self, manifest_url)
            }
            AppAction::PluginRepositoryRemoveRequested { manifest_url } => {
                plugins::dispatch_remove_repository(self, manifest_url)
            }
            AppAction::PluginScraperToggled {
                scraper_id,
                enabled,
            } => plugins::dispatch_toggle_scraper(self, scraper_id, enabled),
            AppAction::PluginScraperSettingsUpdated {
                scraper_id,
                settings,
            } => plugins::dispatch_update_scraper_settings(self, scraper_id, settings),
        }
    }

    fn complete_effect(&mut self, result: EffectResultInput) -> Vec<EffectEnvelope> {
        let Some(effect) = self
            .state
            .pending_effects
            .iter()
            .find(|effect| effect.id == result.effect_id)
            .cloned()
        else {
            return vec![];
        };
        let generation = effect.generation;
        // Unknown effect type (e.g. stale build mismatch between platform and core) — drop silently.
        let Some(kind) = EffectKind::from_str(&effect.kind) else {
            return vec![];
        };
        self.state
            .pending_effects
            .retain(|pending| pending.id != result.effect_id);
        self.delivered_effect_ids.remove(&result.effect_id);
        self.effect_created_at.remove(&result.effect_id);
        let effect_type = kind.as_str();

        // No wildcard arm: adding an EffectKind variant without handling it here is a compile error.
        match kind {
            EffectKind::FetchMetaDetail
            | EffectKind::ReadPlaybackProgress
            | EffectKind::ReadDetailLocalState
            | EffectKind::FetchDetailSecondary
            | EffectKind::PrefetchDetailStreams
            | EffectKind::FetchDetailStreams
            | EffectKind::FetchMetaDetailLookup
            | EffectKind::FetchSeasonEpisodes => {
                detail::complete(self, effect_type, generation, &result)
            }

            EffectKind::LoadStreams
            | EffectKind::StartTorrentStream
            | EffectKind::EnqueueTraktScrobble
            | EffectKind::StopTorrent
            | EffectKind::FetchIntroSegments
            | EffectKind::ResolveIntroImdbId
            | EffectKind::FetchSubtitles
            | EffectKind::PrefetchNextEpisodeStreams => {
                player::complete(self, effect_type, generation, &result)
            }

            EffectKind::ReadHomeBootstrap
            | EffectKind::RefreshContinueWatching
            | EffectKind::PrepareDirectPlayback
            | EffectKind::FetchCatalogPage => {
                home::complete(self, effect_type, generation, &result)
            }

            EffectKind::ReadLibraryState
            | EffectKind::WriteLibraryCommand
            | EffectKind::WriteFeedback
            | EffectKind::ClearPlaybackProgress
            | EffectKind::WritePlaybackProgress
            | EffectKind::SyncWatchedState => {
                library::complete(self, effect_type, generation, &result)
            }

            EffectKind::FetchAddonManifest
            | EffectKind::RefreshInstalledAddons
            | EffectKind::FetchAddonResource => {
                addons::complete(self, effect_type, generation, &result)
            }

            EffectKind::RunSearch => search::complete(self, generation, &result),

            EffectKind::RunDiscover
            | EffectKind::ReadDiscoverCatalogFilters
            | EffectKind::FetchDiscoverPage => {
                discover::complete(self, effect_type, generation, &result)
            }

            EffectKind::ReadCalendarMonth => calendar::complete(self, generation, &result, &effect),

            EffectKind::EnqueueOfflineDownload => offline::complete(self, generation, &result),

            EffectKind::WriteSettings => settings::complete(self, generation, &result),

            EffectKind::RunExternalSync | EffectKind::SyncExternalIntegration => {
                sync::complete(self, effect_type, generation, &result)
            }

            EffectKind::RunAuthFlow
            | EffectKind::ExchangeAuthCode
            | EffectKind::RefreshAuthToken => {
                auth::complete(self, effect_type, generation, &result)
            }

            EffectKind::FetchYoutubeTrailerWatchConfig
            | EffectKind::FetchYoutubeTrailerPlayer
            | EffectKind::FetchYoutubeTrailerPlayerScript => {
                trailer::complete(self, effect_type, generation, &effect, &result)
            }

            EffectKind::FetchPluginManifest => plugins::complete(self, generation, &result),

            EffectKind::UpdateCalendarWidget
            | EffectKind::NotifyReleasedEpisodes
            | EffectKind::ReplaceExternalContinueWatching
            | EffectKind::ExecutePlugin => vec![],
        }
    }

    fn effect<P: serde::Serialize>(
        &mut self,
        kind: EffectKind,
        generation: u64,
        payload: P,
    ) -> EffectEnvelope {
        let payload = serde_json::to_value(&payload).unwrap_or(Value::Null);
        self.effect_raw(kind.as_str(), generation, payload)
    }

    // For pass-through of effects emitted by sub-modules (e.g. player_flow) where
    // the type string is embedded in the JSON at runtime rather than known statically.
    fn effect_raw(&mut self, kind: &str, generation: u64, payload: Value) -> EffectEnvelope {
        let id = format!("fx-{}", self.next_effect_id);
        self.next_effect_id += 1;
        let envelope = EffectEnvelope::raw(id.clone(), kind, generation, payload);
        self.state.pending_effects.push(envelope.clone());
        self.effect_created_at.insert(id, Instant::now());
        envelope
    }

    // Drops any pending effect old enough that it's almost certainly been abandoned by
    // the platform rather than genuinely still in flight. Called opportunistically on
    // every dispatch/complete_effect so no background timer is needed.
    fn expire_stale_pending_effects(&mut self, now: Instant) {
        let stale_ids: Vec<String> = self
            .state
            .pending_effects
            .iter()
            .filter(|effect| {
                self.effect_created_at
                    .get(&effect.id)
                    .is_some_and(|created_at| now.duration_since(*created_at) > EFFECT_EXPIRY)
            })
            .map(|effect| effect.id.clone())
            .collect();
        for id in &stale_ids {
            self.state.pending_effects.retain(|effect| &effect.id != id);
            self.delivered_effect_ids.remove(id);
            self.effect_created_at.remove(id);
        }
    }

    fn bump_generation(&mut self, key: GenerationKey) -> u64 {
        self.state.runtime.bump(key)
    }

    // When a dispatch/complete_effect handler produces no new effects directly, we
    // fall back to draining whatever's still pending so the platform doesn't lose
    // track of multi-effect work spread across several calls. But anything already
    // handed to the platform is presumably still in flight (e.g. an addon fetch that
    // hasn't finished) — redelivering it here would make the platform start a second,
    // duplicate execution of the same effect. Only ever drain genuinely undelivered ones.
    fn resolve_visible_effects(&mut self, effects: Vec<EffectEnvelope>) -> Vec<EffectEnvelope> {
        let visible = if effects.is_empty() {
            self.undelivered_pending_effects()
        } else {
            effects
        };
        for effect in &visible {
            self.delivered_effect_ids.insert(effect.id.clone());
        }
        visible
    }

    fn undelivered_pending_effects(&self) -> Vec<EffectEnvelope> {
        self.state
            .pending_effects
            .iter()
            .filter(|effect| !self.delivered_effect_ids.contains(&effect.id))
            .cloned()
            .collect()
    }
}

// Deliberately takes owned before/after snapshots rather than a reference to the locked
// engine: diffing and serializing a large state (e.g. a big discover catalog) can take
// over a second, and every other Tauri command shares one global engine mutex — holding
// it for that long would stall unrelated IPC calls behind it. Callers clone what they
// need and drop the lock before calling this.
fn result_patch_json(state: StatePatch, effects: Vec<EffectEnvelope>) -> Option<String> {
    serde_json::to_string(&DispatchResult { state, effects }).ok()
}

fn engines() -> &'static Mutex<HashMap<u64, HeadlessEngine>> {
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

// A panic while a request held this lock poisons it; with catch_unwind now
// guarding the FFI boundary, a single caught panic must not silently make
// every engine handle inaccessible for the rest of the process's life.
// Recovering the guard accepts that one engine's state might be left
// mid-update, which is still far better than every other handle going dark.
fn lock_engines() -> std::sync::MutexGuard<'static, HashMap<u64, HeadlessEngine>> {
    engines()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests;
