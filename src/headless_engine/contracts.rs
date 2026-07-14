use crate::runtime::EffectEnvelope;
use crate::types::Profile;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub(super) enum AppAction {
    #[serde(rename = "navigationRequested")]
    NavigationRequested {
        route: String,
        params: Option<Value>,
    },
    #[serde(rename = "detailLoadRequested")]
    DetailLoadRequested {
        content_type: String,
        id: String,
        language: Option<String>,
        source_addon_transport_url: Option<String>,
        source_addon_catalog_type: Option<String>,
        profile: Option<Value>,
    },
    #[serde(rename = "detailLocalStateRequested")]
    DetailLocalStateRequested {
        primary_id: String,
        fallback_id: Option<String>,
        content_type: String,
        profile: Option<Value>,
    },
    #[serde(rename = "detailSecondaryRequested")]
    DetailSecondaryRequested {
        content_type: String,
        id: String,
        language: Option<String>,
        profile: Option<Value>,
    },
    #[serde(rename = "detailPrefetchRequested")]
    DetailPrefetchRequested {
        content_type: String,
        id: String,
        stream_lookup_id: String,
        title: Option<String>,
        original_name: Option<String>,
        year: Option<i32>,
        language: Option<String>,
        profile: Option<Value>,
    },
    #[serde(rename = "detailStreamsRequested")]
    DetailStreamsRequested {
        content_type: String,
        request_ids: Vec<String>,
        detail: Option<Value>,
        season_episodes: Option<Vec<Value>>,
        language: Option<String>,
        profile: Option<Value>,
    },
    #[serde(rename = "detailStreamsAppended")]
    DetailStreamsAppended {
        streams: Vec<Value>,
        available_addons: Vec<String>,
        generation: Option<u64>,
    },
    #[serde(rename = "detailSelectedAddonChanged")]
    DetailSelectedAddonChanged { addon: Option<String> },
    #[serde(rename = "metaDetailRequested")]
    MetaDetailRequested {
        content_type: String,
        id: String,
        language: Option<String>,
        profile: Option<Value>,
    },
    #[serde(rename = "directPlaybackRequested")]
    DirectPlaybackRequested {
        meta: Value,
        language: Option<String>,
        profile: Option<Value>,
    },
    #[serde(rename = "introSegmentsRequested")]
    IntroSegmentsRequested {
        imdb_id: String,
        season: i32,
        episode: i32,
        title: Option<String>,
        use_intro_db: bool,
        use_ani_skip: bool,
    },
    #[serde(rename = "introImdbIdRequested")]
    IntroImdbIdRequested {
        meta: Value,
        video_id: Option<String>,
        language: Option<String>,
    },
    #[serde(rename = "playerLoadStreamsRequested")]
    PlayerLoadStreamsRequested {
        content_type: String,
        id: String,
        current_video_id: Option<String>,
        initial_video_id: Option<String>,
        initial_streams: Option<Vec<Value>>,
        initial_stream_index: Option<i32>,
        saved_url: Option<String>,
        saved_title: Option<String>,
        source_selection_mode: Option<String>,
        regex_pattern: Option<String>,
        preferred_binge_group: Option<String>,
        title: Option<String>,
        original_name: Option<String>,
        year: Option<i32>,
        language: Option<String>,
        profile: Option<Value>,
        outgoing_progress: Option<Value>,
    },
    #[serde(rename = "playerStreamsLoaded")]
    PlayerStreamsLoaded {
        streams: Vec<Value>,
        current_video_id: Option<String>,
        initial_stream_index: Option<i32>,
        saved_url: Option<String>,
        saved_title: Option<String>,
        source_selection_mode: Option<String>,
        regex_pattern: Option<String>,
        preferred_binge_group: Option<String>,
    },
    #[serde(rename = "playerStreamsFailed")]
    PlayerStreamsFailed { error_code: Option<String> },
    #[serde(rename = "playerResolvePlaybackRequested")]
    PlayerResolvePlaybackRequested {
        url: String,
        stream: Option<Value>,
        current_video_id: Option<String>,
        title: Option<String>,
    },
    #[serde(rename = "scrobbleRequested")]
    ScrobbleRequested {
        token: String,
        meta_type: String,
        item_id: String,
        progress: f64,
        action_name: String,
        profile: Option<Value>,
    },
    #[serde(rename = "profileActivated")]
    ProfileActivated { profile: Profile },
    #[serde(rename = "homeLoadRequested")]
    HomeLoadRequested {
        profile: Option<Value>,
        language: Option<String>,
        force: Option<bool>,
    },
    #[serde(rename = "refreshContinueWatchingRequested")]
    RefreshContinueWatchingRequested {
        profile: Option<Value>,
        language: Option<String>,
    },
    #[serde(rename = "libraryHydrateRequested")]
    LibraryHydrateRequested { profile_id: Option<String> },
    #[serde(rename = "toggleWatchlistRequested")]
    ToggleWatchlistRequested { item: Value },
    #[serde(rename = "toggleLibraryStatusRequested")]
    ToggleLibraryStatusRequested { list: String, item: Value },
    #[serde(rename = "setFeedbackRequested")]
    SetFeedbackRequested {
        id: String,
        value: Option<bool>,
        meta: Value,
    },
    #[serde(rename = "clearPlaybackProgressRequested")]
    ClearPlaybackProgressRequested { profile: Option<Value>, meta: Value },
    #[serde(rename = "savePlaybackProgressRequested")]
    SavePlaybackProgressRequested {
        profile: Option<Value>,
        meta: Value,
        time_offset: i64,
        duration: i64,
        last_video_id: Option<String>,
        last_stream_index: Option<i32>,
        last_episode_name: Option<String>,
        last_episode_season: Option<i64>,
        last_episode_number: Option<i64>,
        last_episode_thumbnail: Option<String>,
        last_stream_url: Option<String>,
        last_stream_title: Option<String>,
        last_audio_language: Option<String>,
        last_subtitle_language: Option<String>,
        scrobble_trakt_pause: Option<bool>,
    },
    #[serde(rename = "markWatchedRequested")]
    MarkWatchedRequested {
        series_id: String,
        video_ids: Vec<String>,
        watched: Option<bool>,
        meta: Option<Value>,
        episodes: Option<Vec<Value>>,
        profile: Option<Value>,
    },
    #[serde(rename = "addonInstallRequested")]
    AddonInstallRequested {
        transport_url: String,
        force_refresh: Option<bool>,
    },
    #[serde(rename = "addonsRefreshRequested")]
    AddonsRefreshRequested {
        profile: Option<Value>,
        force_refresh: Option<bool>,
    },
    #[serde(rename = "addonResourceRequested")]
    AddonResourceRequested {
        transport_url: String,
        resource: String,
        content_type: String,
        id: String,
        extra: Option<Value>,
    },
    #[serde(rename = "searchRequested")]
    SearchRequested {
        query: String,
        profile: Option<Value>,
        language: Option<String>,
    },
    #[serde(rename = "discoverRequested")]
    DiscoverRequested {
        content_type: String,
        filters: Option<Value>,
        profile: Option<Value>,
        language: Option<String>,
    },
    #[serde(rename = "discoverCatalogFiltersRequested")]
    DiscoverCatalogFiltersRequested {
        content_type: String,
        selected_catalog_key: Option<String>,
        profile: Option<Value>,
        language: Option<String>,
    },
    #[serde(rename = "discoverPageRequested")]
    DiscoverPageRequested {
        transport_url: Option<String>,
        content_type: String,
        catalog_id: String,
        skip: Option<i32>,
        genre: Option<String>,
    },
    #[serde(rename = "catalogPageRequested")]
    CatalogPageRequested {
        category_id: String,
        transport_url: Option<String>,
        content_type: String,
        catalog_id: String,
        skip: Option<i32>,
        genre: Option<String>,
        search: Option<String>,
        remote_source: Option<Value>,
        profile: Option<Value>,
    },
    #[serde(rename = "detailSeasonRequested")]
    DetailSeasonRequested {
        series_id: String,
        season: i32,
        profile: Option<Value>,
        language: Option<String>,
    },
    #[serde(rename = "playerNextEpisodeCardShown")]
    PlayerNextEpisodeCardShown {
        content_type: String,
        series_id: String,
        next_video_id: String,
        title: Option<String>,
        original_name: Option<String>,
        year: Option<i32>,
        language: Option<String>,
        profile: Option<Value>,
    },
    #[serde(rename = "subtitleLoadRequested")]
    SubtitleLoadRequested {
        stream: Value,
        content_type: String,
        id: String,
        extra_args: Option<String>,
    },
    #[serde(rename = "externalSyncRequested")]
    ExternalSyncRequested {
        provider: String,
        profile: Option<Value>,
        language: Option<String>,
    },
    #[serde(rename = "authFlowRequested")]
    AuthFlowRequested { provider: String, mode: String },
    #[serde(rename = "authExchangeRequested")]
    AuthExchangeRequested {
        provider: String,
        code: String,
        code_verifier: Option<String>,
        profile: Option<Value>,
    },
    #[serde(rename = "authRefreshRequested")]
    AuthRefreshRequested { provider: String, profile: Value },
    #[serde(rename = "externalIntegrationSyncRequested")]
    ExternalIntegrationSyncRequested {
        provider: String,
        profile: Value,
        language: Option<String>,
    },
    #[serde(rename = "settingsChanged")]
    SettingsChanged { key: String, value: Value },
    #[serde(rename = "calendarMonthRequested")]
    CalendarMonthRequested {
        profile: Option<Value>,
        year: i32,
        month: i32,
        #[serde(rename = "plannedItems")]
        planned_items: Option<Value>,
    },
    #[serde(rename = "offlineDownloadRequested")]
    OfflineDownloadRequested {
        meta: Value,
        stream: Value,
        video_id: Option<String>,
        video: Option<Value>,
        subtitle: Option<Value>,
        profile_id: Option<String>,
        language: Option<String>,
    },
    #[serde(rename = "trailerResolveRequested")]
    TrailerResolveRequested { request_id: String, video_id: String },
    #[serde(rename = "trailerPrewarmRequested")]
    TrailerPrewarmRequested,
    #[serde(rename = "pluginRepositoryAddRequested")]
    PluginRepositoryAddRequested { manifest_url: String },
    #[serde(rename = "pluginRepositoryRemoveRequested")]
    PluginRepositoryRemoveRequested { manifest_url: String },
    #[serde(rename = "pluginScraperToggled")]
    PluginScraperToggled { scraper_id: String, enabled: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum EffectStatus {
    Ok,
    Error,
    Cancelled,
}

impl EffectStatus {
    pub(crate) fn is_ok(self) -> bool {
        matches!(self, EffectStatus::Ok)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EffectResultInput {
    pub effect_id: String,
    pub status: EffectStatus,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub error: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DispatchResult {
    pub state: StatePatch,
    pub effects: Vec<EffectEnvelope>,
}

// Only domains that actually changed since the snapshot taken before this dispatch
// are `Some` here — the platform merges this onto its existing state instead of
// replacing it wholesale, since serializing the full EngineState on every action
// scales with everything the user has ever loaded, not with what changed.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StatePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation: Option<super::navigation::NavigationState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<super::home::HomeState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<super::search::SearchState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discover: Option<super::discover::DiscoverState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<super::detail::DetailState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player: Option<super::player::PlayerState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<super::library::LibraryState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<super::profile::ProfileState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<super::settings::SettingsState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar: Option<super::calendar::CalendarState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addons: Option<super::addons::AddonsState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<super::auth::AuthState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync: Option<super::sync::SyncState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lookup: Option<super::detail::LookupState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline: Option<super::offline::OfflineState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailer: Option<super::trailer::TrailerState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<super::plugins::PluginsState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_effects: Option<Vec<EffectEnvelope>>,
}
