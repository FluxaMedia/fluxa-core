use super::helpers::{error_code, normalize_error};
use super::state::GenerationKey;
use super::{EffectResultInput, HeadlessEngine};
use crate::player_flow::{self, PlayerFlowAction, PlayerFlowState};
use crate::runtime::{EffectEnvelope, EffectKind};
use crate::stream_policy;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct PlayerState {
    current_video_id: Value,
    current_streams: Value,
    current_stream_index: i64,
    current_url: Value,
    resolved_url: Value,
    zero_speed_ticks: i64,
    is_buffering: bool,
    is_video_rendered: bool,
    player_error: Value,
    preferred_binge_group: Value,
    pending_stream_load: Value,
    prefetching_next_video_id: Value,
    prefetched_next_episode: Value,
    subtitle_loading: bool,
    subtitles: Value,
    intro_segments: Value,
    intro_imdb_id: Value,
    last_scrobble: Value,
    direct_playback_target: Value,
    stop_torrent_warning: Value,
    generation: u64,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            current_video_id: Value::Null,
            current_streams: serde_json::json!([]),
            current_stream_index: 0,
            current_url: Value::Null,
            resolved_url: Value::Null,
            zero_speed_ticks: 0,
            is_buffering: true,
            is_video_rendered: false,
            player_error: Value::Null,
            preferred_binge_group: Value::Null,
            pending_stream_load: Value::Null,
            prefetching_next_video_id: Value::Null,
            prefetched_next_episode: Value::Null,
            subtitle_loading: false,
            subtitles: serde_json::json!([]),
            intro_segments: serde_json::json!([]),
            intro_imdb_id: Value::Null,
            last_scrobble: Value::Null,
            direct_playback_target: Value::Null,
            stop_torrent_warning: Value::Null,
            generation: 0,
        }
    }
}

impl PlayerState {
    fn to_flow_state(&self) -> PlayerFlowState {
        PlayerFlowState {
            current_video_id: self.current_video_id.as_str().map(str::to_string),
            current_streams: self.current_streams.as_array().cloned().unwrap_or_default(),
            current_stream_index: self.current_stream_index as i32,
            current_url: self.current_url.as_str().map(str::to_string),
            zero_speed_ticks: self.zero_speed_ticks as i32,
            is_buffering: self.is_buffering,
            is_video_rendered: self.is_video_rendered,
            player_error: self.player_error.as_str().map(str::to_string),
            preferred_binge_group: self.preferred_binge_group.as_str().map(str::to_string),
        }
    }

    // The player_flow sub-engine owns only the playback-selection fields (current
    // video/streams/url/etc). Applying its result wholesale replaces the player
    // namespace, dropping every headless-level extension field (pendingStreamLoad,
    // prefetch cache, subtitles, ...) back to default — this mirrors the previous
    // behavior of overwriting `engine.state["player"]` with the flow's state outright.
    fn from_flow_state(flow_state: PlayerFlowState) -> Self {
        Self {
            current_video_id: flow_state
                .current_video_id
                .map(Value::String)
                .unwrap_or(Value::Null),
            current_streams: Value::Array(flow_state.current_streams),
            current_stream_index: flow_state.current_stream_index as i64,
            current_url: flow_state
                .current_url
                .map(Value::String)
                .unwrap_or(Value::Null),
            zero_speed_ticks: flow_state.zero_speed_ticks as i64,
            is_buffering: flow_state.is_buffering,
            is_video_rendered: flow_state.is_video_rendered,
            player_error: flow_state
                .player_error
                .map(Value::String)
                .unwrap_or(Value::Null),
            preferred_binge_group: flow_state
                .preferred_binge_group
                .map(Value::String)
                .unwrap_or(Value::Null),
            ..Self::default()
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrefetchNextEpisodeStreamsPayload {
    content_type: String,
    series_id: String,
    next_video_id: String,
    title: String,
    original_name: Option<String>,
    year: Option<i32>,
    language: String,
    profile: Value,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingStreamLoad {
    saved_url: Option<String>,
    saved_title: Option<String>,
    source_selection_mode: String,
    regex_pattern: Option<String>,
    preferred_binge_group: Option<String>,
    initial_streams: Vec<Value>,
    initial_stream_index: i32,
    current_video_id: Option<String>,
    title: Option<String>,
    original_name: Option<String>,
    year: Option<i32>,
    language: String,
    profile: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartTorrentStreamPayload {
    url: String,
    stream: Value,
    current_video_id: Option<String>,
    title: String,
    file_idx: Option<i64>,
    preferred_filename: Option<String>,
    sources: Vec<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StopTorrentPayload {
    reason: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnqueueTraktScrobblePayload {
    token: String,
    meta_type: String,
    item_id: String,
    progress: f64,
    action_name: String,
    profile: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchIntroSegmentsPayload {
    imdb_id: String,
    season: i32,
    episode: i32,
    title: Option<String>,
    use_intro_db: bool,
    use_ani_skip: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolveIntroImdbIdPayload {
    meta: Value,
    video_id: Option<String>,
    language: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchSubtitlesPayload {
    stream: Value,
    content_type: String,
    id: String,
    extra_args: String,
}

pub(super) fn complete_direct_playback(engine: &mut HeadlessEngine, value: Value, error: Value) {
    if error.is_null() {
        engine.state.player.direct_playback_target = value;
        engine.state.player.player_error = Value::Null;
    } else {
        engine.state.player.direct_playback_target = Value::Null;
        engine.state.player.player_error = error;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_next_episode_prefetch(
    engine: &mut HeadlessEngine,
    content_type: String,
    series_id: String,
    next_video_id: String,
    title: Option<String>,
    original_name: Option<String>,
    year: Option<i32>,
    language: Option<String>,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let already_prefetching = engine
        .state
        .player
        .prefetching_next_video_id
        .as_str()
        .is_some_and(|v| v == next_video_id);
    let already_cached = engine.state.player.prefetched_next_episode["videoId"]
        .as_str()
        .is_some_and(|v| v == next_video_id);
    if already_prefetching || already_cached {
        return vec![];
    }

    let generation = engine.state.runtime.get(GenerationKey::Player);
    engine.state.player.prefetching_next_video_id = Value::String(next_video_id.clone());
    vec![engine.effect(
        EffectKind::PrefetchNextEpisodeStreams,
        generation,
        PrefetchNextEpisodeStreamsPayload {
            content_type,
            series_id,
            next_video_id,
            title: title.unwrap_or_default(),
            original_name,
            year,
            language: language.unwrap_or_else(|| "en".to_string()),
            profile: profile.unwrap_or(Value::Null),
        },
    )]
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_load_streams(
    engine: &mut HeadlessEngine,
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
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Player);
    let mut initial_streams = initial_streams.unwrap_or_default();
    let initial_stream_index = initial_stream_index.unwrap_or(0);

    // If no initial streams were provided by the caller but we have a prefetch
    // cache hit for this video_id, inject those streams so playback can start
    // without waiting for a fresh fetch.
    let mut effective_initial_video_id = initial_video_id.clone();
    if initial_streams.is_empty() {
        let prefetched = engine.state.player.prefetched_next_episode.clone();
        let cached_video_id = prefetched["videoId"].as_str().map(str::to_string);
        if cached_video_id.is_some() && cached_video_id == current_video_id {
            initial_streams = prefetched["streams"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            effective_initial_video_id = cached_video_id;
            engine.state.player.prefetched_next_episode = Value::Null;
        }
    }

    let action = PlayerFlowAction::LoadStreamsRequested {
        content_type: content_type.clone(),
        id: id.clone(),
        current_video_id: current_video_id.clone(),
        initial_video_id: effective_initial_video_id,
        initial_streams: initial_streams.clone(),
        initial_stream_index,
    };
    let mut flow_state = engine.state.player.to_flow_state();
    let effects = player_flow::dispatch(&mut flow_state, action);
    engine.state.player = PlayerState::from_flow_state(flow_state);

    let pending = PendingStreamLoad {
        saved_url,
        saved_title,
        source_selection_mode: source_selection_mode.unwrap_or_else(|| "manual".to_string()),
        regex_pattern,
        preferred_binge_group,
        initial_streams,
        initial_stream_index,
        current_video_id,
        title,
        original_name,
        year,
        language: language.unwrap_or_else(|| "en".to_string()),
        profile: profile.unwrap_or(Value::Null),
    };
    let pending_value = serde_json::to_value(&pending).unwrap_or(Value::Null);
    engine.state.player.pending_stream_load = pending_value.clone();

    effects
        .into_iter()
        .map(|effect| {
            let mut payload = serde_json::to_value(&effect).unwrap_or(Value::Null);
            let kind = payload
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            if kind == "loadStreams" {
                if let Value::Object(map) = &mut payload {
                    map.insert(
                        "initialStreams".to_string(),
                        pending_value["initialStreams"].clone(),
                    );
                    map.insert("title".to_string(), pending_value["title"].clone());
                    map.insert(
                        "originalName".to_string(),
                        pending_value["originalName"].clone(),
                    );
                    map.insert("year".to_string(), pending_value["year"].clone());
                    map.insert("language".to_string(), pending_value["language"].clone());
                    map.insert("profile".to_string(), pending_value["profile"].clone());
                }
            }
            engine.effect_raw(&kind, generation, payload)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_streams_loaded(
    engine: &mut HeadlessEngine,
    streams: Vec<Value>,
    current_video_id: Option<String>,
    initial_stream_index: Option<i32>,
    saved_url: Option<String>,
    saved_title: Option<String>,
    source_selection_mode: Option<String>,
    regex_pattern: Option<String>,
    preferred_binge_group: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = engine.state.runtime.get(GenerationKey::Player);
    let action = PlayerFlowAction::StreamsLoaded {
        streams,
        current_video_id,
        initial_stream_index: initial_stream_index.unwrap_or(0),
        saved_url,
        saved_title,
        source_selection_mode,
        regex_pattern,
        preferred_binge_group,
    };
    let mut flow_state = engine.state.player.to_flow_state();
    let _ = player_flow::dispatch(&mut flow_state, action);
    engine.state.player = PlayerState::from_flow_state(flow_state);
    engine.state.player.generation = generation;
    vec![]
}

pub(super) fn dispatch_streams_failed(
    engine: &mut HeadlessEngine,
    err_code: Option<String>,
) -> Vec<EffectEnvelope> {
    let action = PlayerFlowAction::StreamsFailed {
        error_code: err_code,
    };
    let mut flow_state = engine.state.player.to_flow_state();
    let _ = player_flow::dispatch(&mut flow_state, action);
    engine.state.player = PlayerState::from_flow_state(flow_state);
    vec![]
}

pub(super) fn dispatch_resolve_playback(
    engine: &mut HeadlessEngine,
    url: String,
    stream: Option<Value>,
    current_video_id: Option<String>,
    title: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Player);
    engine.state.player.current_url = Value::String(url.clone());
    engine.state.player.resolved_url = Value::Null;
    engine.state.player.is_buffering = true;
    engine.state.player.is_video_rendered = false;
    engine.state.player.player_error = Value::Null;
    if stream_policy::is_torrent_playback_url(&url) {
        let stream_value = stream.unwrap_or(Value::Null);
        let file_idx = stream_value["fileIdx"].as_i64();
        let preferred_filename = stream_value["effectiveFilename"]
            .as_str()
            .map(ToString::to_string);
        let sources = stream_value["sources"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        vec![engine.effect(
            EffectKind::StartTorrentStream,
            generation,
            StartTorrentStreamPayload {
                url,
                stream: stream_value,
                current_video_id,
                title: title.unwrap_or_else(|| "Fluxa".to_string()),
                file_idx,
                preferred_filename,
                sources,
            },
        )]
    } else {
        engine.state.player.resolved_url = engine.state.player.current_url.clone();
        engine.state.player.is_buffering = false;
        vec![engine.effect(
            EffectKind::StopTorrent,
            generation,
            StopTorrentPayload {
                reason: "directPlayback",
            },
        )]
    }
}

pub(super) fn dispatch_scrobble(
    engine: &mut HeadlessEngine,
    token: String,
    meta_type: String,
    item_id: String,
    progress: f64,
    action_name: String,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Player);
    vec![engine.effect(
        EffectKind::EnqueueTraktScrobble,
        generation,
        EnqueueTraktScrobblePayload {
            token,
            meta_type,
            item_id,
            progress,
            action_name,
            profile: profile.unwrap_or(Value::Null),
        },
    )]
}

pub(super) fn dispatch_intro_segments(
    engine: &mut HeadlessEngine,
    imdb_id: String,
    season: i32,
    episode: i32,
    title: Option<String>,
    use_intro_db: bool,
    use_ani_skip: bool,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Intro);
    vec![engine.effect(
        EffectKind::FetchIntroSegments,
        generation,
        FetchIntroSegmentsPayload {
            imdb_id,
            season,
            episode,
            title,
            use_intro_db,
            use_ani_skip,
        },
    )]
}

pub(super) fn dispatch_intro_imdb_id(
    engine: &mut HeadlessEngine,
    meta: Value,
    video_id: Option<String>,
    language: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Intro);
    vec![engine.effect(
        EffectKind::ResolveIntroImdbId,
        generation,
        ResolveIntroImdbIdPayload {
            meta,
            video_id,
            language: language.unwrap_or_else(|| "en".to_string()),
        },
    )]
}

pub(super) fn dispatch_subtitle_load(
    engine: &mut HeadlessEngine,
    stream: Value,
    content_type: String,
    id: String,
    extra_args: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Player);
    engine.state.player.subtitle_loading = true;
    vec![engine.effect(
        EffectKind::FetchSubtitles,
        generation,
        FetchSubtitlesPayload {
            stream,
            content_type,
            id,
            extra_args: extra_args.unwrap_or_default(),
        },
    )]
}

pub(super) fn complete(
    engine: &mut HeadlessEngine,
    effect_type: &str,
    generation: u64,
    result: &EffectResultInput,
) -> Vec<EffectEnvelope> {
    match effect_type {
        "loadStreams" => {
            if generation == engine.state.runtime.get(GenerationKey::Player) {
                let pending = engine.state.player.pending_stream_load.clone();
                if result.status == "ok" {
                    dispatch_streams_loaded(
                        engine,
                        result.value.as_array().cloned().unwrap_or_default(),
                        pending["currentVideoId"].as_str().map(ToString::to_string),
                        Some(pending["initialStreamIndex"].as_i64().unwrap_or(0) as i32),
                        pending["savedUrl"].as_str().map(ToString::to_string),
                        pending["savedTitle"].as_str().map(ToString::to_string),
                        pending["sourceSelectionMode"]
                            .as_str()
                            .map(ToString::to_string),
                        pending["regexPattern"].as_str().map(ToString::to_string),
                        pending["preferredBingeGroup"]
                            .as_str()
                            .map(ToString::to_string),
                    );
                } else {
                    dispatch_streams_failed(engine, Some(error_code(&result.error)));
                }
                engine.state.player.pending_stream_load = Value::Null;
            }
        }
        "startTorrentStream" => {
            if generation == engine.state.runtime.get(GenerationKey::Player) {
                if result.status == "ok" {
                    engine.state.player.resolved_url =
                        result.value.get("url").cloned().unwrap_or(Value::Null);
                    engine.state.player.is_buffering = false;
                    engine.state.player.player_error = Value::Null;
                } else {
                    engine.state.player.resolved_url = Value::Null;
                    engine.state.player.is_buffering = false;
                    engine.state.player.player_error = Value::String(error_code(&result.error));
                }
            }
        }
        "enqueueTraktScrobble" => {
            if generation == engine.state.runtime.get(GenerationKey::Player) {
                if result.status == "ok" {
                    engine.state.player.last_scrobble = result.value.clone();
                    engine.state.player.player_error = Value::Null;
                } else {
                    engine.state.player.player_error = Value::String(error_code(&result.error));
                }
            }
        }
        "stopTorrent" => {
            if generation == engine.state.runtime.get(GenerationKey::Player)
                && result.status != "ok"
            {
                engine.state.player.stop_torrent_warning = normalize_error(result.error.clone());
            }
        }
        "fetchIntroSegments" => {
            if generation == engine.state.runtime.get(GenerationKey::Intro) {
                if result.status == "ok" {
                    engine.state.player.intro_segments = result.value.clone();
                    engine.state.player.player_error = Value::Null;
                } else {
                    engine.state.player.intro_segments = serde_json::json!([]);
                    engine.state.player.player_error = Value::String(error_code(&result.error));
                }
            }
        }
        "resolveIntroImdbId" => {
            if generation == engine.state.runtime.get(GenerationKey::Intro) {
                if result.status == "ok" {
                    engine.state.player.intro_imdb_id = result.value.clone();
                    engine.state.player.player_error = Value::Null;
                } else {
                    engine.state.player.intro_imdb_id = Value::Null;
                    engine.state.player.player_error = Value::String(error_code(&result.error));
                }
            }
        }
        "fetchSubtitles" => {
            if generation == engine.state.runtime.get(GenerationKey::Player) {
                engine.state.player.subtitle_loading = false;
                if result.status == "ok" {
                    engine.state.player.subtitles = result
                        .value
                        .get("subtitles")
                        .cloned()
                        .unwrap_or_else(|| result.value.clone());
                    engine.state.player.player_error = Value::Null;
                } else {
                    engine.state.player.player_error = Value::String(error_code(&result.error));
                }
            }
        }
        "prefetchNextEpisodeStreams" => {
            // Only accept if the prefetch generation is still current (player hasn't moved on).
            if generation == engine.state.runtime.get(GenerationKey::Player) {
                if result.status == "ok" {
                    let prefetched_video_id = engine.state.player.prefetching_next_video_id.clone();
                    engine.state.player.prefetched_next_episode = serde_json::json!({
                        "videoId": prefetched_video_id,
                        "streams": result.value.get("streams").cloned().unwrap_or_else(|| serde_json::json!([]))
                    });
                }
                engine.state.player.prefetching_next_video_id = Value::Null;
            }
        }
        _ => {}
    }
    vec![]
}
