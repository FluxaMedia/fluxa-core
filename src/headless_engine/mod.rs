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
mod search;
mod settings;
mod state;
mod sync;

use crate::runtime::{EffectEnvelope, EffectKind};
use contracts::{AppAction, DispatchResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use state::default_state;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

pub(crate) use contracts::EffectResultInput;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeadlessEngine {
    #[serde(default)]
    state: Value,
    #[serde(default)]
    next_effect_id: u64,
}

impl Default for HeadlessEngine {
    fn default() -> Self {
        Self {
            state: default_state(),
            next_effect_id: 1,
        }
    }
}

static ENGINE_COUNTER: AtomicU64 = AtomicU64::new(1);
static ENGINES: OnceLock<Mutex<HashMap<u64, HeadlessEngine>>> = OnceLock::new();

pub(crate) fn create_headless_engine(initial_json: &str) -> u64 {
    let mut engine = HeadlessEngine::default();
    if let Ok(initial_state) = serde_json::from_str::<Value>(initial_json) {
        helpers::merge_object(&mut engine.state, initial_state);
    }
    if !engine.state["pendingEffects"].is_array() {
        engine.state["pendingEffects"] = json!([]);
    }
    if let Ok(mut map) = engines().lock() {
        let handle = ENGINE_COUNTER.fetch_add(1, Ordering::Relaxed);
        map.insert(handle, engine);
        handle
    } else {
        0
    }
}

pub(crate) fn destroy_headless_engine(handle: u64) -> bool {
    engines()
        .lock()
        .ok()
        .and_then(|mut map| map.remove(&handle))
        .is_some()
}

pub(crate) fn headless_engine_snapshot_json(handle: u64) -> Option<String> {
    let map = engines().lock().ok()?;
    serde_json::to_string(&map.get(&handle)?.state).ok()
}

pub(crate) fn headless_engine_dispatch_json(handle: u64, action_json: &str) -> Option<String> {
    let action: AppAction = serde_json::from_str(action_json).ok()?;
    let mut map = engines().lock().ok()?;
    let engine = map.get_mut(&handle)?;
    let effects = engine.dispatch(action);
    engine.result_json(effects)
}

pub(crate) fn headless_engine_complete_effect_json(
    handle: u64,
    result_json: &str,
) -> Option<String> {
    let result: EffectResultInput = serde_json::from_str(result_json).ok()?;
    let mut map = engines().lock().ok()?;
    let engine = map.get_mut(&handle)?;
    let effects = engine.complete_effect(result);
    engine.result_json(effects)
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
            } => detail::dispatch_load(self, content_type, id, language, source_addon_transport_url, source_addon_catalog_type, profile),
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
            } => detail::dispatch_secondary(self, content_type, id, language, profile),
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
            } => detail::dispatch_streams_appended(self, streams, available_addons),
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
            AppAction::LibraryHydrateRequested { profile_id } => {
                library::dispatch_hydrate(self, profile_id)
            }
            AppAction::ToggleWatchlistRequested { item } => {
                library::dispatch_toggle_watchlist(self, item)
            }
            AppAction::SetFeedbackRequested { id, value, meta } => {
                library::dispatch_set_feedback(self, id, value, meta)
            }
            AppAction::ClearPlaybackProgressRequested { profile, meta } => {
                library::dispatch_clear_progress(self, profile, meta)
            }
            AppAction::SavePlaybackProgressRequested {
                profile,
                meta,
                time_offset,
                duration,
                last_video_id,
                last_stream_index,
                last_episode_name,
                last_episode_season,
                last_episode_number,
                last_episode_thumbnail,
                last_stream_url,
                last_stream_title,
                last_audio_language,
                last_subtitle_language,
                scrobble_trakt_pause,
            } => library::dispatch_save_progress(
                self,
                profile,
                meta,
                time_offset,
                duration,
                last_video_id,
                last_stream_index,
                last_episode_name,
                last_episode_season,
                last_episode_number,
                last_episode_thumbnail,
                last_stream_url,
                last_stream_title,
                last_audio_language,
                last_subtitle_language,
                scrobble_trakt_pause,
            ),
            AppAction::MarkWatchedRequested {
                series_id,
                video_ids,
                watched,
                meta,
                episodes,
                profile,
            } => library::dispatch_mark_watched(
                self, series_id, video_ids, watched, meta, episodes, profile,
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
            AppAction::CatalogPageRequested {
                category_id,
                transport_url,
                content_type,
                catalog_id,
                skip,
                genre,
                search,
            } => home::dispatch_catalog_page(
                self,
                category_id,
                transport_url,
                content_type,
                catalog_id,
                skip,
                genre,
                search,
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
        }
    }

    fn complete_effect(&mut self, result: EffectResultInput) -> Vec<EffectEnvelope> {
        let effect = match helpers::pending_effect(&self.state, &result.effect_id) {
            Some(effect) => effect,
            None => return vec![],
        };
        let generation = effect["generation"].as_u64().unwrap_or(0);
        let effect_type = effect["type"].as_str().unwrap_or_default().to_string();
        helpers::remove_pending_effect(&mut self.state, &result.effect_id);

        match effect_type.as_str() {
            "fetchMetaDetail"
            | "readPlaybackProgress"
            | "readDetailLocalState"
            | "fetchDetailSecondary"
            | "prefetchDetailStreams"
            | "fetchDetailStreams"
            | "fetchMetaDetailLookup"
            | "fetchSeasonEpisodes" => detail::complete(self, &effect_type, generation, &result),

            "loadStreams"
            | "startTorrentStream"
            | "enqueueTraktScrobble"
            | "stopTorrent"
            | "fetchIntroSegments"
            | "resolveIntroImdbId"
            | "fetchSubtitles"
            | "prefetchNextEpisodeStreams" => player::complete(self, &effect_type, generation, &result),

            "readHomeBootstrap" | "prepareDirectPlayback" | "fetchCatalogPage" => {
                home::complete(self, &effect_type, generation, &result)
            }

            "readLibraryState"
            | "writeLibraryCommand"
            | "writeFeedback"
            | "clearPlaybackProgress"
            | "writePlaybackProgress"
            | "syncWatchedState" => library::complete(self, &effect_type, generation, &result),

            "fetchAddonManifest" | "refreshInstalledAddons" | "fetchAddonResource" => {
                addons::complete(self, &effect_type, generation, &result)
            }

            "runSearch" => search::complete(self, generation, &result),

            "runDiscover" | "readDiscoverCatalogFilters" => {
                discover::complete(self, &effect_type, generation, &result)
            }

            "readCalendarMonth" => calendar::complete(self, generation, &result, &effect),

            "enqueueOfflineDownload" => offline::complete(self, generation, &result),

            "writeSettings" => settings::complete(self, generation, &result),

            "runExternalSync" | "syncExternalIntegration" => {
                sync::complete(self, &effect_type, generation, &result)
            }

            "runAuthFlow" | "exchangeAuthCode" | "refreshAuthToken" => {
                auth::complete(self, &effect_type, generation, &result)
            }

            "updateCalendarWidget"
            | "notifyReleasedEpisodes"
            | "replaceExternalContinueWatching" => {
                vec![]
            }

            _ => vec![],
        }
    }

    fn effect(&mut self, kind: EffectKind, generation: u64, payload: Value) -> EffectEnvelope {
        self.effect_raw(kind.as_str(), generation, payload)
    }

    // For pass-through of effects emitted by sub-modules (e.g. player_flow) where
    // the type string is embedded in the JSON at runtime rather than known statically.
    fn effect_raw(&mut self, kind: &str, generation: u64, payload: Value) -> EffectEnvelope {
        let id = format!("fx-{}", self.next_effect_id);
        self.next_effect_id += 1;
        let envelope = EffectEnvelope::raw(id, kind, generation, payload);
        if let Ok(as_value) = serde_json::to_value(&envelope) {
            if let Some(arr) = self.state["pendingEffects"].as_array_mut() {
                arr.push(as_value);
            }
        }
        envelope
    }

    fn bump_generation(&mut self, key: &str) -> u64 {
        let next = helpers::current_generation(&self.state, key).saturating_add(1);
        self.state["_runtime"][key] = json!(next);
        next
    }

    fn result_json(&self, effects: Vec<EffectEnvelope>) -> Option<String> {
        // When a complete_effect handler produces no new effects, we return all
        // remaining pendingEffects so the platform can drain the queue in one pass.
        let visible_effects = if effects.is_empty() {
            self.state["pendingEffects"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect()
        } else {
            effects
        };
        serde_json::to_string(&DispatchResult {
            state: self.state.clone(),
            effects: visible_effects,
        })
        .ok()
    }
}

fn engines() -> &'static Mutex<HashMap<u64, HeadlessEngine>> {
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_load_emits_platform_effects_and_completion_updates_state() {
        let handle = create_headless_engine("{}");
        let result: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1","language":"en"}"#,
            )
            .expect("dispatch"),
        )
        .expect("json");

        assert_eq!(result["state"]["detail"]["isLoading"], true);
        assert_eq!(result["effects"][0]["type"], "fetchMetaDetail");
        assert_eq!(result["effects"][1]["type"], "readPlaybackProgress");

        let effect_id = result["effects"][0]["id"].as_str().unwrap();
        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": effect_id,
                    "status": "ok",
                    "value": { "id": "tt1", "name": "Movie" }
                })
                .to_string(),
            )
            .expect("complete"),
        )
        .expect("json");

        assert_eq!(completed["state"]["detail"]["isLoading"], false);
        assert_eq!(completed["state"]["detail"]["meta"]["name"], "Movie");
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn detail_meta_trailers_are_normalized_in_core_before_tmdb_fallback() {
        let handle = create_headless_engine("{}");
        let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1","language":"en"}"#,
            )
            .expect("dispatch"),
        )
        .expect("json");
        let effect_id = requested["effects"][0]["id"].as_str().unwrap();
        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": effect_id,
                    "status": "ok",
                    "value": {
                        "id": "tt1",
                        "name": "Movie",
                        "trailers": [
                            { "source": "abc123", "type": "Trailer" },
                            { "title": "Featurette", "url": "https://video.example/f.mp4", "type": "Clip" }
                        ]
                    }
                })
                .to_string(),
            )
            .expect("complete"),
        )
        .expect("json");

        assert_eq!(
            completed["state"]["detail"]["trailers"][0]["url"],
            "https://www.youtube.com/watch?v=abc123"
        );
        assert_eq!(
            completed["state"]["detail"]["trailers"][1]["title"],
            "Featurette"
        );

        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn detail_selected_addon_changes_visible_streams_without_mutating_raw_streams() {
        let handle = create_headless_engine("{}");
        let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailStreamsRequested","contentType":"movie","requestIds":["tt1"],"detail":null,"seasonEpisodes":[],"language":"en"}"#,
            )
            .expect("dispatch"),
        )
        .expect("json");
        let effect_id = requested["effects"][0]["id"].as_str().unwrap();
        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": effect_id,
                    "status": "ok",
                    "value": {
                        "streams": [
                            { "title": "A", "addonName": "One" },
                            { "title": "B", "addonName": "Two" },
                            { "title": "C", "addonName": "One" }
                        ],
                        "availableAddons": ["One", "Two"],
                        "hasStreamProviders": true
                    }
                })
                .to_string(),
            )
            .expect("complete"),
        )
        .expect("json");
        assert_eq!(completed["state"]["detail"]["streams"][0]["title"], "A");
        assert_eq!(
            completed["state"]["detail"]["visibleStreams"][1]["title"],
            "B"
        );

        let selected: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailSelectedAddonChanged","addon":"one"}"#,
            )
            .expect("dispatch"),
        )
        .expect("json");

        assert_eq!(
            selected["state"]["detail"]["streams"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            selected["state"]["detail"]["visibleStreams"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            selected["state"]["detail"]["visibleStreams"][0]["title"],
            "A"
        );
        assert_eq!(
            selected["state"]["detail"]["visibleStreams"][1]["title"],
            "C"
        );
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn stale_detail_effect_completion_does_not_override_newer_state() {
        let handle = create_headless_engine("{}");
        let first: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let stale_effect_id = first["effects"][0]["id"].as_str().unwrap().to_string();

        headless_engine_dispatch_json(
            handle,
            r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt2"}"#,
        )
        .unwrap();

        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": stale_effect_id,
                    "status": "ok",
                    "value": { "id": "tt1", "name": "Old" }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(completed["state"]["detail"]["id"], "tt2");
        assert!(completed["state"]["detail"]["meta"].is_null());
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn player_load_streams_uses_effect_completion_without_reordering_streams() {
        let handle = create_headless_engine("{}");
        let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerLoadStreamsRequested","contentType":"movie","id":"tt1","currentVideoId":"tt1","initialStreamIndex":1}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(requested["effects"][0]["type"], "loadStreams");

        let effect_id = requested["effects"][0]["id"].as_str().unwrap();
        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": effect_id,
                    "status": "ok",
                    "value": [
                        { "title": "A", "playableUrl": "http://a" },
                        { "title": "B", "playableUrl": "http://b" }
                    ]
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(completed["state"]["player"]["currentStreamIndex"], 1);
        assert_eq!(completed["state"]["player"]["currentUrl"], "http://b");
        assert_eq!(
            completed["state"]["player"]["currentStreams"][0]["title"],
            "A"
        );
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn player_resolve_playback_emits_torrent_or_direct_platform_effects() {
        let handle = create_headless_engine("{}");
        let torrent: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerResolvePlaybackRequested","url":"stremio://torrent/abc","stream":{"title":"T"},"currentVideoId":"tt1","title":"Movie"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(torrent["effects"][0]["type"], "startTorrentStream");
        let effect_id = torrent["effects"][0]["id"].as_str().unwrap();

        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": effect_id,
                    "status": "ok",
                    "value": { "url": "http://127.0.0.1:8090/stream" }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            completed["state"]["player"]["resolvedUrl"],
            "http://127.0.0.1:8090/stream"
        );

        let direct: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerResolvePlaybackRequested","url":"https://video.example/file.mp4","title":"Movie"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            direct["state"]["player"]["resolvedUrl"],
            "https://video.example/file.mp4"
        );
        assert_eq!(direct["effects"][0]["type"], "stopTorrent");
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn home_load_is_owned_by_core_and_resolved_through_platform_effect() {
        let handle = create_headless_engine("{}");
        let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"homeLoadRequested","profile":{"id":"p1"},"language":"tr","force":true}"#,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(requested["state"]["home"]["isLoading"], true);
        assert_eq!(requested["effects"][0]["type"], "readHomeBootstrap");
        assert_eq!(requested["effects"][0]["payload"]["profileId"], "p1");
        assert_eq!(requested["effects"][0]["payload"]["language"], "tr");
        assert_eq!(requested["effects"][0]["payload"]["force"], true);

        let effect_id = requested["effects"][0]["id"].as_str().unwrap();
        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": effect_id,
                    "status": "ok",
                    "value": {
                        "categories": [{ "id": "featured" }],
                        "continueWatching": [{ "id": "tt1" }],
                        "metadataFeeds": [{ "key": "cinemeta" }],
                        "billboard": { "id": "tt2" }
                    }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(completed["state"]["home"]["isLoading"], false);
        assert_eq!(
            completed["state"]["home"]["categories"][0]["id"],
            "featured"
        );
        assert_eq!(
            completed["state"]["home"]["continueWatching"][0]["id"],
            "tt1"
        );
        assert_eq!(completed["state"]["home"]["billboard"]["id"], "tt2");
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn library_commands_are_storage_effects_owned_by_core() {
        let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);
        let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"toggleWatchlistRequested","item":{"id":"tt1","name":"Movie","type":"movie"}}"#,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(requested["effects"][0]["type"], "writeLibraryCommand");
        assert_eq!(requested["effects"][0]["payload"]["profileId"], "p1");
        assert_eq!(
            requested["effects"][0]["payload"]["command"]["type"],
            "toggleWatchlist"
        );
        assert_eq!(
            requested["effects"][0]["payload"]["command"]["item"]["id"],
            "tt1"
        );

        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                    "status": "ok",
                    "value": { "watchlist": [{ "id": "tt1" }] }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        assert!(completed["state"]["library"]["lastWriteError"].is_null());
        assert_eq!(
            completed["state"]["library"]["lastWrite"]["watchlist"][0]["id"],
            "tt1"
        );
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn playback_progress_write_is_clamped_and_delegated_to_storage_adapter() {
        let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);
        let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"savePlaybackProgressRequested","meta":{"id":"tt1","name":"Movie","type":"movie"},"timeOffset":-10,"duration":7200,"lastVideoId":"tt1","lastStreamIndex":2,"lastEpisodeName":null,"lastStreamUrl":"http://a","lastStreamTitle":"A","lastAudioLanguage":"en","lastSubtitleLanguage":"tr"}"#,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(requested["effects"][0]["type"], "writePlaybackProgress");
        assert_eq!(requested["effects"][0]["payload"]["profileId"], "p1");
        assert_eq!(
            requested["effects"][0]["payload"]["progress"]["timeOffset"],
            0
        );
        assert_eq!(
            requested["effects"][0]["payload"]["progress"]["lastSubtitleLanguage"],
            "tr"
        );

        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                    "status": "ok",
                    "value": {}
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        assert!(completed["state"]["library"]["pendingPlaybackProgress"].is_null());
        assert_eq!(
            completed["state"]["library"]["savedPlaybackProgress"]["meta"]["id"],
            "tt1"
        );
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn effect_completion_returns_remaining_pending_effects_for_adapter_drain() {
        let handle = create_headless_engine("{}");
        let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1","language":"en"}"#,
            )
            .unwrap(),
        )
        .unwrap();

        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                    "status": "ok",
                    "value": { "id": "tt1", "name": "Movie" }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(completed["effects"][0]["type"], "readPlaybackProgress");
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn addon_search_discover_and_catalog_backbone_are_effect_driven() {
        let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);

        let addon: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"addonInstallRequested","transportUrl":"https://addon.example/manifest.json","forceRefresh":true}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(addon["effects"][0]["type"], "fetchAddonManifest");
        assert_eq!(
            addon["effects"][0]["payload"]["transportUrl"],
            "https://addon.example/manifest.json"
        );

        let completed_addon: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": addon["effects"][0]["id"].as_str().unwrap(),
                    "status": "ok",
                    "value": {
                        "id": "addon.example",
                        "transportUrl": "https://addon.example/manifest.json",
                        "name": "Addon"
                    }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            completed_addon["state"]["addons"]["installed"][0]["name"],
            "Addon"
        );

        let resource: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"addonResourceRequested","transportUrl":"https://addon.example/manifest.json","resource":"stream","contentType":"movie","id":"tt1","extra":{"search":"keep order"}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(resource["effects"][0]["type"], "fetchAddonResource");
        assert_eq!(resource["effects"][0]["payload"]["resource"], "stream");
        assert_eq!(
            resource["effects"][0]["payload"]["extra"]["search"],
            "keep order"
        );

        let search: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"searchRequested","query":"matrix","language":"en"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(search["effects"][0]["type"], "runSearch");
        assert_eq!(search["effects"][0]["payload"]["profileId"], "p1");

        let discover: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"discoverRequested","contentType":"movie","filters":{"genre":"sci-fi"},"language":"en"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(discover["effects"][0]["type"], "runDiscover");
        assert_eq!(
            discover["effects"][0]["payload"]["filters"]["genre"],
            "sci-fi"
        );

        let page: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"catalogPageRequested","categoryId":"cat","transportUrl":"https://addon.example/manifest.json","contentType":"movie","catalogId":"top","skip":-10,"genre":null,"search":null}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(page["effects"][0]["type"], "fetchCatalogPage");
        assert_eq!(page["effects"][0]["payload"]["skip"], 0);
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn detail_player_sync_auth_settings_calendar_and_offline_are_core_actions() {
        let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);

        let season: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailSeasonRequested","seriesId":"tt1","season":2}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(season["effects"][0]["type"], "fetchSeasonEpisodes");

        let subtitles: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"subtitleLoadRequested","stream":{"url":"http://a"},"contentType":"movie","id":"tt1","extraArgs":"videoHash=abc"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(subtitles["effects"][0]["type"], "fetchSubtitles");
        assert_eq!(
            subtitles["effects"][0]["payload"]["extraArgs"],
            "videoHash=abc"
        );

        let sync: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"externalSyncRequested","provider":"trakt","language":"tr"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(sync["effects"][0]["type"], "runExternalSync");
        assert_eq!(sync["effects"][0]["payload"]["profileId"], "p1");

        let auth: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"authFlowRequested","provider":"trakt","mode":"deviceCode"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(auth["effects"][0]["type"], "runAuthFlow");

        let settings: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"settingsChanged","key":"language","value":"tr"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(settings["state"]["settings"]["values"]["language"], "tr");
        assert_eq!(settings["effects"][0]["type"], "writeSettings");

        let calendar: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"calendarMonthRequested","year":2026,"month":20}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(calendar["effects"][0]["type"], "readCalendarMonth");
        assert_eq!(calendar["effects"][0]["payload"]["month"], 12);

        let offline: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"offlineDownloadRequested","meta":{"id":"tt1"},"stream":{"url":"http://a"},"videoId":"tt1"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(offline["effects"][0]["type"], "enqueueOfflineDownload");
        assert_eq!(offline["effects"][0]["payload"]["meta"]["id"], "tt1");
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn calendar_completion_plans_os_side_effects_in_core() {
        let handle = create_headless_engine("{}");
        let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"calendarMonthRequested","profile":{"id":"p1","language":"tr"},"year":2026,"month":5}"#,
            )
            .unwrap(),
        )
        .unwrap();

        let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                    "status": "ok",
                    "value": {
                        "items": [{ "dateIso": "2026-05-20", "title": "Episode" }],
                        "localItems": [{ "id": "tt1" }],
                        "externalItems": [{ "id": "tt2" }]
                    }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(completed["state"]["calendar"]["isLoading"], false);
        assert_eq!(
            completed["state"]["calendar"]["items"][0]["title"],
            "Episode"
        );
        assert_eq!(
            completed["effects"]
                .as_array()
                .unwrap()
                .iter()
                .map(|effect| effect["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "updateCalendarWidget",
                "notifyReleasedEpisodes",
                "replaceExternalContinueWatching"
            ]
        );
        assert_eq!(completed["effects"][0]["payload"]["profile"]["id"], "p1");
        assert_eq!(completed["effects"][2]["payload"]["items"][0]["id"], "tt2");
        assert!(destroy_headless_engine(handle));
    }

    #[test]
    fn next_episode_card_shown_prefetches_streams_and_load_streams_consumes_cache() {
        let handle = create_headless_engine("{}");

        // 1. Next episode card shown for episode tt1:1:2
        let prefetch_requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerNextEpisodeCardShown","contentType":"series","seriesId":"tt1","nextVideoId":"tt1:1:2","title":"Show","language":"en"}"#,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(prefetch_requested["effects"][0]["type"], "prefetchNextEpisodeStreams");
        assert_eq!(prefetch_requested["effects"][0]["payload"]["nextVideoId"], "tt1:1:2");
        assert_eq!(prefetch_requested["state"]["player"]["prefetchingNextVideoId"], "tt1:1:2");

        // Duplicate card-shown dispatch must not change prefetching state.
        let duplicate: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerNextEpisodeCardShown","contentType":"series","seriesId":"tt1","nextVideoId":"tt1:1:2"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        // Guard works: prefetchingNextVideoId unchanged, no new prefetch effect was queued.
        assert_eq!(duplicate["state"]["player"]["prefetchingNextVideoId"], "tt1:1:2");

        // 2. Platform completes the prefetch with streams for tt1:1:2
        let effect_id = prefetch_requested["effects"][0]["id"].as_str().unwrap();
        let prefetch_done: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": effect_id,
                    "status": "ok",
                    "value": {
                        "streams": [
                            { "title": "S", "playableUrl": "http://ep2" }
                        ]
                    }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(prefetch_done["state"]["player"]["prefetchedNextEpisode"]["videoId"], "tt1:1:2");
        assert_eq!(prefetch_done["state"]["player"]["prefetchedNextEpisode"]["streams"][0]["title"], "S");
        assert!(prefetch_done["state"]["player"]["prefetchingNextVideoId"].is_null());

        // 3. User navigates to ep2 — load streams without passing initial_streams.
        //    Core must inject the prefetched streams and use_initial_streams = true.
        let load: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerLoadStreamsRequested","contentType":"series","id":"tt1","currentVideoId":"tt1:1:2"}"#,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(load["effects"][0]["type"], "loadStreams");
        // useInitialStreams = true means the platform skips the network fetch
        assert_eq!(load["effects"][0]["payload"]["useInitialStreams"], true);
        // Cache must be consumed (cleared) after use
        assert!(load["state"]["player"]["prefetchedNextEpisode"].is_null());

        assert!(destroy_headless_engine(handle));
    }
}
