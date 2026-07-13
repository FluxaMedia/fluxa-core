use serde::{Deserialize, Serialize};

/// Exhaustive catalog of all effect types the headless engine can emit.
///
/// This is the single source of truth for effect type names — the string
/// representations produced by `as_str()` are the ones the platform (Kotlin,
/// JS, etc.) matches against in its effect dispatcher.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    ClearPlaybackProgress,
    EnqueueOfflineDownload,
    EnqueueTraktScrobble,
    ExchangeAuthCode,
    ExecutePlugin,
    FetchAddonManifest,
    FetchAddonResource,
    FetchCatalogPage,
    FetchDiscoverPage,
    FetchDetailSecondary,
    FetchDetailStreams,
    FetchIntroSegments,
    FetchMetaDetail,
    FetchMetaDetailLookup,
    FetchPluginManifest,
    FetchSeasonEpisodes,
    FetchSubtitles,
    FetchYoutubeTrailerPlayer,
    FetchYoutubeTrailerWatchConfig,
    LoadStreams,
    NotifyReleasedEpisodes,
    PrefetchDetailStreams,
    PrefetchNextEpisodeStreams,
    PrepareDirectPlayback,
    ReadCalendarMonth,
    ReadDetailLocalState,
    ReadDiscoverCatalogFilters,
    ReadHomeBootstrap,
    RefreshContinueWatching,
    ReadLibraryState,
    ReadPlaybackProgress,
    RefreshAuthToken,
    RefreshInstalledAddons,
    ReplaceExternalContinueWatching,
    ResolveIntroImdbId,
    RunAuthFlow,
    RunDiscover,
    RunExternalSync,
    RunSearch,
    StartTorrentStream,
    StopTorrent,
    SyncExternalIntegration,
    SyncWatchedState,
    UpdateCalendarWidget,
    WriteFeedback,
    WriteLibraryCommand,
    WritePlaybackProgress,
    WriteSettings,
}

impl EffectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EffectKind::ClearPlaybackProgress => "clearPlaybackProgress",
            EffectKind::EnqueueOfflineDownload => "enqueueOfflineDownload",
            EffectKind::EnqueueTraktScrobble => "enqueueTraktScrobble",
            EffectKind::ExchangeAuthCode => "exchangeAuthCode",
            EffectKind::ExecutePlugin => "executePlugin",
            EffectKind::FetchAddonManifest => "fetchAddonManifest",
            EffectKind::FetchAddonResource => "fetchAddonResource",
            EffectKind::FetchCatalogPage => "fetchCatalogPage",
            EffectKind::FetchDiscoverPage => "fetchDiscoverPage",
            EffectKind::FetchDetailSecondary => "fetchDetailSecondary",
            EffectKind::FetchDetailStreams => "fetchDetailStreams",
            EffectKind::FetchIntroSegments => "fetchIntroSegments",
            EffectKind::FetchMetaDetail => "fetchMetaDetail",
            EffectKind::FetchMetaDetailLookup => "fetchMetaDetailLookup",
            EffectKind::FetchPluginManifest => "fetchPluginManifest",
            EffectKind::FetchSeasonEpisodes => "fetchSeasonEpisodes",
            EffectKind::FetchSubtitles => "fetchSubtitles",
            EffectKind::FetchYoutubeTrailerPlayer => "fetchYoutubeTrailerPlayer",
            EffectKind::FetchYoutubeTrailerWatchConfig => "fetchYoutubeTrailerWatchConfig",
            EffectKind::LoadStreams => "loadStreams",
            EffectKind::NotifyReleasedEpisodes => "notifyReleasedEpisodes",
            EffectKind::PrefetchDetailStreams => "prefetchDetailStreams",
            EffectKind::PrefetchNextEpisodeStreams => "prefetchNextEpisodeStreams",
            EffectKind::PrepareDirectPlayback => "prepareDirectPlayback",
            EffectKind::ReadCalendarMonth => "readCalendarMonth",
            EffectKind::ReadDetailLocalState => "readDetailLocalState",
            EffectKind::ReadDiscoverCatalogFilters => "readDiscoverCatalogFilters",
            EffectKind::ReadHomeBootstrap => "readHomeBootstrap",
            EffectKind::RefreshContinueWatching => "refreshContinueWatching",
            EffectKind::ReadLibraryState => "readLibraryState",
            EffectKind::ReadPlaybackProgress => "readPlaybackProgress",
            EffectKind::RefreshAuthToken => "refreshAuthToken",
            EffectKind::RefreshInstalledAddons => "refreshInstalledAddons",
            EffectKind::ReplaceExternalContinueWatching => "replaceExternalContinueWatching",
            EffectKind::ResolveIntroImdbId => "resolveIntroImdbId",
            EffectKind::RunAuthFlow => "runAuthFlow",
            EffectKind::RunDiscover => "runDiscover",
            EffectKind::RunExternalSync => "runExternalSync",
            EffectKind::RunSearch => "runSearch",
            EffectKind::StartTorrentStream => "startTorrentStream",
            EffectKind::StopTorrent => "stopTorrent",
            EffectKind::SyncExternalIntegration => "syncExternalIntegration",
            EffectKind::SyncWatchedState => "syncWatchedState",
            EffectKind::UpdateCalendarWidget => "updateCalendarWidget",
            EffectKind::WriteFeedback => "writeFeedback",
            EffectKind::WriteLibraryCommand => "writeLibraryCommand",
            EffectKind::WritePlaybackProgress => "writePlaybackProgress",
            EffectKind::WriteSettings => "writeSettings",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "clearPlaybackProgress" => EffectKind::ClearPlaybackProgress,
            "enqueueOfflineDownload" => EffectKind::EnqueueOfflineDownload,
            "enqueueTraktScrobble" => EffectKind::EnqueueTraktScrobble,
            "exchangeAuthCode" => EffectKind::ExchangeAuthCode,
            "fetchAddonManifest" => EffectKind::FetchAddonManifest,
            "fetchAddonResource" => EffectKind::FetchAddonResource,
            "fetchCatalogPage" => EffectKind::FetchCatalogPage,
            "fetchDiscoverPage" => EffectKind::FetchDiscoverPage,
            "fetchDetailSecondary" => EffectKind::FetchDetailSecondary,
            "fetchDetailStreams" => EffectKind::FetchDetailStreams,
            "fetchIntroSegments" => EffectKind::FetchIntroSegments,
            "fetchMetaDetail" => EffectKind::FetchMetaDetail,
            "fetchMetaDetailLookup" => EffectKind::FetchMetaDetailLookup,
            "fetchSeasonEpisodes" => EffectKind::FetchSeasonEpisodes,
            "fetchSubtitles" => EffectKind::FetchSubtitles,
            "fetchYoutubeTrailerPlayer" => EffectKind::FetchYoutubeTrailerPlayer,
            "fetchYoutubeTrailerWatchConfig" => EffectKind::FetchYoutubeTrailerWatchConfig,
            "loadStreams" => EffectKind::LoadStreams,
            "notifyReleasedEpisodes" => EffectKind::NotifyReleasedEpisodes,
            "prefetchDetailStreams" => EffectKind::PrefetchDetailStreams,
            "prefetchNextEpisodeStreams" => EffectKind::PrefetchNextEpisodeStreams,
            "prepareDirectPlayback" => EffectKind::PrepareDirectPlayback,
            "readCalendarMonth" => EffectKind::ReadCalendarMonth,
            "readDetailLocalState" => EffectKind::ReadDetailLocalState,
            "readDiscoverCatalogFilters" => EffectKind::ReadDiscoverCatalogFilters,
            "readHomeBootstrap" => EffectKind::ReadHomeBootstrap,
            "refreshContinueWatching" => EffectKind::RefreshContinueWatching,
            "readLibraryState" => EffectKind::ReadLibraryState,
            "readPlaybackProgress" => EffectKind::ReadPlaybackProgress,
            "refreshAuthToken" => EffectKind::RefreshAuthToken,
            "refreshInstalledAddons" => EffectKind::RefreshInstalledAddons,
            "replaceExternalContinueWatching" => EffectKind::ReplaceExternalContinueWatching,
            "resolveIntroImdbId" => EffectKind::ResolveIntroImdbId,
            "runAuthFlow" => EffectKind::RunAuthFlow,
            "runDiscover" => EffectKind::RunDiscover,
            "runExternalSync" => EffectKind::RunExternalSync,
            "runSearch" => EffectKind::RunSearch,
            "startTorrentStream" => EffectKind::StartTorrentStream,
            "stopTorrent" => EffectKind::StopTorrent,
            "syncExternalIntegration" => EffectKind::SyncExternalIntegration,
            "syncWatchedState" => EffectKind::SyncWatchedState,
            "updateCalendarWidget" => EffectKind::UpdateCalendarWidget,
            "writeFeedback" => EffectKind::WriteFeedback,
            "writeLibraryCommand" => EffectKind::WriteLibraryCommand,
            "writePlaybackProgress" => EffectKind::WritePlaybackProgress,
            "writeSettings" => EffectKind::WriteSettings,
            _ => return None,
        })
    }
}

/// Wire format for an effect emitted by the headless engine.
///
/// Matches the `NativeHeadlessEffect` data class on the Kotlin side:
/// ```kotlin
/// data class NativeHeadlessEffect(
///     val id: String,
///     val type: String,
///     val generation: Long,
///     val payload: Map<String, Any?>
/// )
/// ```
///
/// `id` is a monotonically-increasing opaque string (`"fx-N"`).
/// `generation` lets the platform discard stale completions.
/// `payload` carries effect-specific parameters as a JSON object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectEnvelope {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub generation: u64,
    pub payload: serde_json::Value,
}

impl EffectEnvelope {
    pub fn new(id: String, kind: EffectKind, generation: u64, payload: serde_json::Value) -> Self {
        Self {
            id,
            kind: kind.as_str().to_owned(),
            generation,
            payload,
        }
    }

    pub fn raw(id: String, kind: &str, generation: u64, payload: serde_json::Value) -> Self {
        Self {
            id,
            kind: kind.to_owned(),
            generation,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EffectKind;

    #[test]
    fn as_str_and_from_str_roundtrip_for_every_variant() {
        let all = [
            EffectKind::ClearPlaybackProgress,
            EffectKind::EnqueueOfflineDownload,
            EffectKind::EnqueueTraktScrobble,
            EffectKind::ExchangeAuthCode,
            EffectKind::FetchAddonManifest,
            EffectKind::FetchAddonResource,
            EffectKind::FetchCatalogPage,
            EffectKind::FetchDiscoverPage,
            EffectKind::FetchDetailSecondary,
            EffectKind::FetchDetailStreams,
            EffectKind::FetchIntroSegments,
            EffectKind::FetchMetaDetail,
            EffectKind::FetchMetaDetailLookup,
            EffectKind::FetchSeasonEpisodes,
            EffectKind::FetchSubtitles,
            EffectKind::FetchYoutubeTrailerPlayer,
            EffectKind::FetchYoutubeTrailerWatchConfig,
            EffectKind::LoadStreams,
            EffectKind::NotifyReleasedEpisodes,
            EffectKind::PrefetchDetailStreams,
            EffectKind::PrefetchNextEpisodeStreams,
            EffectKind::PrepareDirectPlayback,
            EffectKind::ReadCalendarMonth,
            EffectKind::ReadDetailLocalState,
            EffectKind::ReadDiscoverCatalogFilters,
            EffectKind::ReadHomeBootstrap,
            EffectKind::ReadLibraryState,
            EffectKind::ReadPlaybackProgress,
            EffectKind::RefreshAuthToken,
            EffectKind::RefreshInstalledAddons,
            EffectKind::ReplaceExternalContinueWatching,
            EffectKind::ResolveIntroImdbId,
            EffectKind::RunAuthFlow,
            EffectKind::RunDiscover,
            EffectKind::RunExternalSync,
            EffectKind::RunSearch,
            EffectKind::StartTorrentStream,
            EffectKind::StopTorrent,
            EffectKind::SyncExternalIntegration,
            EffectKind::SyncWatchedState,
            EffectKind::UpdateCalendarWidget,
            EffectKind::WriteFeedback,
            EffectKind::WriteLibraryCommand,
            EffectKind::WritePlaybackProgress,
            EffectKind::WriteSettings,
        ];
        for kind in all {
            assert_eq!(EffectKind::from_str(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn from_str_rejects_unknown_value() {
        assert_eq!(EffectKind::from_str("notAnEffect"), None);
    }
}
