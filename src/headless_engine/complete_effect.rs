use super::HeadlessEngine;
use super::contracts::EffectResultInput;
use super::{
    addons, auth, calendar, detail, discover, home, library, offline, player, plugins, search,
    settings, sync, trailer,
};
use crate::runtime::{EffectEnvelope, EffectKind};

impl HeadlessEngine {
    pub(super) fn complete_effect(&mut self, result: EffectResultInput) -> Vec<EffectEnvelope> {
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
}
