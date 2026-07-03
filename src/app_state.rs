use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCoreState {
    #[serde(default)]
    pub home: HomeState,
    #[serde(default)]
    pub home_search: HomeSearchState,
    #[serde(default)]
    pub billboard: BillboardState,
    #[serde(default)]
    pub discover: DiscoverState,
    #[serde(default)]
    pub calendar: CalendarState,
    #[serde(default)]
    pub library: LibraryState,
    #[serde(default)]
    pub player: PlayerCoreState,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillboardState {
    #[serde(default)]
    pub error: Value,
    #[serde(default)]
    pub pool: Value,
    #[serde(default)]
    pub index: i64,
    #[serde(default)]
    pub movie: Value,
    #[serde(default)]
    pub logo: Value,
    #[serde(default)]
    pub watchlist: bool,
    #[serde(default)]
    pub next_episode: Value,
    #[serde(default)]
    pub trailer_url: Value,
}

impl Default for BillboardState {
    fn default() -> Self {
        Self {
            error: Value::Null,
            pool: json!([]),
            index: 0,
            movie: Value::Null,
            logo: Value::Null,
            watchlist: false,
            next_episode: Value::Null,
            trailer_url: Value::Null,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverState {
    #[serde(default)]
    pub results: Value,
    #[serde(default)]
    pub is_loading: bool,
    #[serde(default)]
    pub genres: Value,
    #[serde(default)]
    pub catalogs: Value,
}

impl Default for DiscoverState {
    fn default() -> Self {
        Self {
            results: json!([]),
            is_loading: false,
            genres: json!([]),
            catalogs: json!([]),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarState {
    #[serde(default)]
    pub items: Value,
    #[serde(default)]
    pub is_loading: bool,
}

impl Default for CalendarState {
    fn default() -> Self {
        Self {
            items: json!([]),
            is_loading: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryState {
    #[serde(default)]
    pub ui_state: Value,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            ui_state: json!({}),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeState {
    #[serde(default)]
    pub categories: Value,
    #[serde(default)]
    pub is_loading: bool,
    #[serde(default = "default_home_filter")]
    pub current_filter: String,
    #[serde(default)]
    pub is_direct_loading: bool,
    #[serde(default)]
    pub trakt_continue_watching_last_updated_at: i64,
    #[serde(default)]
    pub user_addons: Value,
    #[serde(default)]
    pub watchlist: Value,
    #[serde(default)]
    pub liked_items: Value,
    #[serde(default)]
    pub active_profile: Value,
    #[serde(default)]
    pub current_watchlist: Value,
    #[serde(default)]
    pub external_continue_watching: Value,
    #[serde(default)]
    pub trakt_watched_state: Value,
}

impl Default for HomeState {
    fn default() -> Self {
        Self {
            categories: json!([]),
            is_loading: false,
            current_filter: default_home_filter(),
            is_direct_loading: false,
            trakt_continue_watching_last_updated_at: 0,
            user_addons: json!([]),
            watchlist: json!([]),
            liked_items: json!([]),
            active_profile: Value::Null,
            current_watchlist: json!([]),
            external_continue_watching: json!([]),
            trakt_watched_state: json!({}),
        }
    }
}

fn default_home_filter() -> String {
    "all".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeSearchState {
    #[serde(default)]
    pub search_results: Value,
    #[serde(default)]
    pub search_rows: Value,
    #[serde(default)]
    pub search_history: Value,
    #[serde(default)]
    pub focused_movie: Value,
    #[serde(default)]
    pub focused_movie_trailer_url: Value,
    #[serde(default)]
    pub preview_url: Value,
}

impl Default for HomeSearchState {
    fn default() -> Self {
        Self {
            search_results: json!([]),
            search_rows: json!([]),
            search_history: json!([]),
            focused_movie: Value::Null,
            focused_movie_trailer_url: Value::Null,
            preview_url: Value::Null,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerCoreState {
    #[serde(default)]
    pub current_video_id: Value,
    #[serde(default)]
    pub current_stream_index: i64,
    #[serde(default)]
    pub last_saved_position: i64,
    #[serde(default)]
    pub should_apply_initial_progress: bool,
    #[serde(default)]
    pub playback_ended: bool,
    #[serde(default)]
    pub has_started_playing: bool,
    #[serde(default)]
    pub is_video_rendered: bool,
    #[serde(default = "default_buffering")]
    pub is_buffering: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl Default for PlayerCoreState {
    fn default() -> Self {
        Self {
            current_video_id: Value::Null,
            current_stream_index: 0,
            last_saved_position: 0,
            should_apply_initial_progress: false,
            playback_ended: false,
            has_started_playing: false,
            is_video_rendered: false,
            is_buffering: true,
            extra: HashMap::new(),
        }
    }
}

fn default_buffering() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppCoreAction {
    #[serde(rename = "type")]
    action_type: String,
    #[serde(default)]
    value: Value,
    #[serde(default)]
    video_id: Value,
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static STORE: OnceLock<Mutex<HashMap<u64, AppCoreState>>> = OnceLock::new();

fn store() -> &'static Mutex<HashMap<u64, AppCoreState>> {
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

// See headless_engine::lock_engines — recovering from poison keeps this store
// usable after a single caught panic instead of going dark for every handle.
fn lock_store() -> std::sync::MutexGuard<'static, HashMap<u64, AppCoreState>> {
    store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn create_app_core_state(initial_json: &str) -> u64 {
    let state = serde_json::from_str(initial_json).unwrap_or_default();
    let mut states = lock_store();
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    states.insert(handle, state);
    handle
}

pub fn destroy_app_core_state(handle: u64) -> bool {
    lock_store().remove(&handle).is_some()
}

pub fn app_core_state_json(handle: u64) -> Option<String> {
    let states = lock_store();
    states
        .get(&handle)
        .and_then(|state| serde_json::to_string(state).ok())
}

pub fn app_core_dispatch_json(handle: u64, action_json: &str) -> Option<String> {
    let action: AppCoreAction = serde_json::from_str(action_json).ok()?;
    let mut states = lock_store();
    let state = states.get_mut(&handle)?;
    reduce(state, action);
    serde_json::to_string(state).ok()
}

fn reduce(state: &mut AppCoreState, action: AppCoreAction) {
    match action.action_type.as_str() {
        "setHomeCategories" => state.home.categories = array_or_empty(action.value),
        "setHomeLoading" => state.home.is_loading = action.value.as_bool().unwrap_or(false),
        "setHomeCurrentFilter" => {
            state.home.current_filter = action
                .value
                .as_str()
                .filter(|value| !value.is_empty())
                .unwrap_or("all")
                .to_string()
        }
        "setHomeDirectLoading" => {
            state.home.is_direct_loading = action.value.as_bool().unwrap_or(false)
        }
        "setTraktContinueWatchingLastUpdatedAt" => {
            state.home.trakt_continue_watching_last_updated_at = action.value.as_i64().unwrap_or(0)
        }
        "setUserAddons" => state.home.user_addons = array_or_empty(action.value),
        "setWatchlist" => state.home.watchlist = array_or_empty(action.value),
        "setLikedItems" => state.home.liked_items = array_or_empty(action.value),
        "setActiveProfile" => state.home.active_profile = action.value,
        "setCurrentWatchlist" => state.home.current_watchlist = array_or_empty(action.value),
        "setExternalContinueWatching" => {
            state.home.external_continue_watching = array_or_empty(action.value)
        }
        "setTraktWatchedState" => state.home.trakt_watched_state = action.value,
        "setSearchResults" => state.home_search.search_results = array_or_empty(action.value),
        "setSearchRows" => state.home_search.search_rows = array_or_empty(action.value),
        "setSearchHistory" => state.home_search.search_history = array_or_empty(action.value),
        "setFocusedMovie" => state.home_search.focused_movie = action.value,
        "setFocusedMovieTrailerUrl" => state.home_search.focused_movie_trailer_url = action.value,
        "setPreviewUrl" => state.home_search.preview_url = action.value,
        "setBillboardError" => state.billboard.error = action.value,
        "setBillboardPool" => state.billboard.pool = array_or_empty(action.value),
        "setBillboardIndex" => state.billboard.index = action.value.as_i64().unwrap_or(0).max(0),
        "setBillboardMovie" => state.billboard.movie = action.value,
        "setBillboardLogo" => state.billboard.logo = action.value,
        "setBillboardWatchlist" => {
            state.billboard.watchlist = action.value.as_bool().unwrap_or(false)
        }
        "setBillboardNextEpisode" => state.billboard.next_episode = action.value,
        "setBillboardTrailerUrl" => state.billboard.trailer_url = action.value,
        "setDiscoverResults" => state.discover.results = array_or_empty(action.value),
        "setDiscoverLoading" => state.discover.is_loading = action.value.as_bool().unwrap_or(false),
        "setDiscoverGenres" => state.discover.genres = array_or_empty(action.value),
        "setDiscoverCatalogs" => state.discover.catalogs = array_or_empty(action.value),
        "setCalendarItems" => state.calendar.items = array_or_empty(action.value),
        "setCalendarLoading" => state.calendar.is_loading = action.value.as_bool().unwrap_or(false),
        "setLibraryUiState" => state.library.ui_state = action.value,
        "playerResetForEpisode" => reset_player_for_episode(&mut state.player, action.video_id),
        _ => {}
    }
}

fn array_or_empty(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        json!([])
    }
}

fn reset_player_for_episode(player: &mut PlayerCoreState, video_id: Value) {
    player.current_video_id = video_id;
    player.current_stream_index = 0;
    player.last_saved_position = 0;
    player.should_apply_initial_progress = false;
    player.playback_ended = false;
    player.has_started_playing = false;
    player.is_video_rendered = false;
    player.is_buffering = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reducer_updates_home_search_state_without_reordering_payloads() {
        let handle = create_app_core_state(
            r#"{"home":{"categories":[{"id":"c1"}]},"homeSearch":{"searchHistory":[{"id":"tt1"}]}}"#,
        );

        let snapshot = app_core_dispatch_json(
            handle,
            r#"{"type":"setSearchResults","value":[{"id":"tt2"},{"id":"tt1"}]}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();

        assert_eq!(
            value["homeSearch"]["searchResults"],
            json!([{"id":"tt2"},{"id":"tt1"}])
        );
        assert_eq!(value["home"]["categories"], json!([{"id":"c1"}]));
        assert_eq!(value["homeSearch"]["searchHistory"], json!([{"id":"tt1"}]));
        assert!(destroy_app_core_state(handle));
    }

    #[test]
    fn reducer_owns_home_shell_state() {
        let handle = create_app_core_state("{}");

        let snapshot = app_core_dispatch_json(
            handle,
            r#"{"type":"setHomeCategories","value":[{"id":"continue_watching"},{"id":"popular"}]}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(
            value["home"]["categories"],
            json!([{"id":"continue_watching"},{"id":"popular"}])
        );

        let snapshot = app_core_dispatch_json(
            handle,
            r#"{"type":"setHomeCurrentFilter","value":"movies"}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(value["home"]["currentFilter"], json!("movies"));

        let snapshot =
            app_core_dispatch_json(handle, r#"{"type":"setHomeLoading","value":true}"#).unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(value["home"]["isLoading"], json!(true));
        assert_eq!(value["home"]["currentFilter"], json!("movies"));
        assert!(destroy_app_core_state(handle));
    }

    #[test]
    fn reducer_owns_home_feature_state_branches() {
        let handle = create_app_core_state("{}");

        let snapshot =
            app_core_dispatch_json(handle, r#"{"type":"setBillboardIndex","value":2}"#).unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(value["billboard"]["index"], json!(2));

        let snapshot = app_core_dispatch_json(
            handle,
            r#"{"type":"setDiscoverResults","value":[{"id":"tt1"},{"id":"tt2"}]}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(
            value["discover"]["results"],
            json!([{"id":"tt1"},{"id":"tt2"}])
        );

        let snapshot = app_core_dispatch_json(
            handle,
            r#"{"type":"setCalendarItems","value":[{"title":"Episode"}]}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(value["calendar"]["items"], json!([{"title":"Episode"}]));

        let snapshot = app_core_dispatch_json(
            handle,
            r#"{"type":"setLibraryUiState","value":{"isLoading":false,"lastLoadedProfileKey":"profile"}}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(
            value["library"]["uiState"]["lastLoadedProfileKey"],
            json!("profile")
        );
        assert!(destroy_app_core_state(handle));
    }

    #[test]
    fn reducer_resets_player_episode_state_like_kotlin_state_holder() {
        let handle = create_app_core_state(
            r#"{"player":{"currentStreamIndex":3,"lastSavedPosition":9200,"playbackEnded":true,"hasStartedPlaying":true,"isVideoRendered":true,"isBuffering":false}}"#,
        );

        let snapshot = app_core_dispatch_json(
            handle,
            r#"{"type":"playerResetForEpisode","videoId":"tt123:1:2"}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&snapshot).unwrap();

        assert_eq!(value["player"]["currentVideoId"], json!("tt123:1:2"));
        assert_eq!(value["player"]["currentStreamIndex"], json!(0));
        assert_eq!(value["player"]["lastSavedPosition"], json!(0));
        assert_eq!(value["player"]["shouldApplyInitialProgress"], json!(false));
        assert_eq!(value["player"]["playbackEnded"], json!(false));
        assert_eq!(value["player"]["hasStartedPlaying"], json!(false));
        assert_eq!(value["player"]["isVideoRendered"], json!(false));
        assert_eq!(value["player"]["isBuffering"], json!(true));
        assert!(destroy_app_core_state(handle));
    }
}
