use crate::action_contract::{
    mark_watched_action_value, save_playback_progress_action_value, MarkWatchedAction,
    SavePlaybackProgressAction,
};
use crate::core_error::{CoreError, LogAndDiscard};
use crate::library_state::{UP_NEXT_DURATION_SECONDS, UP_NEXT_POSITION_SECONDS};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendSelectionRequest {
    #[serde(default)]
    stream: Value,
    #[serde(default)]
    preferred_player: Option<String>,
    #[serde(default)]
    device_has_dolby_vision_decoder: bool,
    #[serde(default)]
    device_has_hdr_display: bool,
    #[serde(default)]
    force_software_audio: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TorrentFallbackRequest {
    #[serde(default)]
    file_stats: Vec<Value>,
    #[serde(default)]
    rejected_index: Option<i32>,
    #[serde(default)]
    video_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BufferTargetsRequest {
    #[serde(default)]
    forward_buffer_seconds: Option<i64>,
    #[serde(default)]
    back_buffer_seconds: Option<i64>,
    #[serde(default)]
    cache_size_mb: Option<i64>,
    #[serde(default)]
    is_torrent: bool,
    #[serde(default)]
    mobile_data_usage: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetryPolicyRequest {
    error_code: String,
    #[serde(default)]
    retry_count: i32,
    #[serde(default)]
    is_torrent: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceSidebarRequest {
    #[serde(default)]
    streams: Vec<Value>,
    #[serde(default)]
    current_stream_index: i32,
    #[serde(default)]
    available_addons: Vec<String>,
    #[serde(default)]
    selected_addon: Option<String>,
}

pub(crate) fn player_backend_selection_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<BackendSelectionRequest>(request_json)
        .map_err(|e| CoreError::BadInput {
            context: "player_backend_selection_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let preferred = request.preferred_player.as_deref().unwrap_or("internal");
    let stream = &request.stream;

    let url = stream
        .get("playableUrl")
        .or_else(|| stream.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let is_external_player_url = url.starts_with("intent://")
        || stream
            .get("externalPlayerUrl")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || preferred == "external";

    if is_external_player_url {
        return serde_json::to_string(&json!({
            "backend": "external",
            "reason": "external_player_preference"
        }))
        .ok();
    }

    // MPV is preferred for:
    // - HDR / Dolby Vision streams when device doesn't have native HW decoder
    // - Streams that specify mpv hints
    // - User explicitly chose MPV
    let has_mpv_hint = stream
        .get("behaviorHints")
        .and_then(|h| h.get("preferMpv"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let is_dv_stream = stream.get("dv").and_then(Value::as_bool).unwrap_or(false)
        || stream
            .get("dolbyVision")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let is_hdr_stream = stream.get("hdr").and_then(Value::as_bool).unwrap_or(false);
    let needs_mpv_for_hdr = (is_dv_stream && !request.device_has_dolby_vision_decoder)
        || (is_hdr_stream && !request.device_has_hdr_display);

    let use_mpv = preferred == "mpv"
        || has_mpv_hint
        || needs_mpv_for_hdr
        || (request.force_software_audio && preferred != "exoplayer");

    let backend = if use_mpv { "mpv" } else { "exoplayer" };
    let reason = if preferred == "mpv" || preferred == "exoplayer" {
        "user_preference"
    } else if has_mpv_hint {
        "stream_hint"
    } else if needs_mpv_for_hdr {
        "hdr_no_hw_decoder"
    } else if request.force_software_audio {
        "software_audio"
    } else {
        "default"
    };

    serde_json::to_string(&json!({
        "backend": backend,
        "reason": reason
    }))
    .ok()
}

pub(crate) fn torrent_fallback_file_policy_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<TorrentFallbackRequest>(request_json)
        .map_err(|e| CoreError::BadInput {
            context: "torrent_fallback_file_policy_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let rejected = request.rejected_index;
    let video_id = request.video_id.as_deref().unwrap_or("");

    // Collect video-likely files (by extension)
    let video_exts = [
        ".mkv", ".mp4", ".avi", ".mov", ".wmv", ".flv", ".webm", ".m4v",
    ];
    let mut candidates: Vec<(i32, i64)> = request
        .file_stats
        .iter()
        .filter_map(|stat| {
            let id = stat.get("id").and_then(Value::as_i64)? as i32;
            if rejected == Some(id) {
                return None;
            }
            let path = stat
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            let is_video = video_exts.iter().any(|ext| path.ends_with(ext));
            if !is_video {
                return None;
            }
            let length = stat.get("length").and_then(Value::as_i64).unwrap_or(0);
            // Skip tiny files (less than 1MB) unless it's the only candidate
            if length < 1_000_000 && request.file_stats.len() > 1 {
                return None;
            }
            Some((id, length))
        })
        .collect();

    // Sort by size descending (largest first as most likely the right video file)
    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    // If we have a video_id hint, try to match by episode pattern
    let fallback_ids: Vec<i32> = if !video_id.is_empty() {
        // Episode-matched first, then size-sorted remainder
        let mut matched: Vec<(i32, i64)> = Vec::new();
        let mut unmatched: Vec<(i32, i64)> = Vec::new();
        for (id, length) in &candidates {
            let path = request
                .file_stats
                .iter()
                .find(|s| s.get("id").and_then(Value::as_i64) == Some(*id as i64))
                .and_then(|s| s.get("path").and_then(Value::as_str))
                .unwrap_or("");
            if episode_path_matches_id(path, video_id) {
                matched.push((*id, *length));
            } else {
                unmatched.push((*id, *length));
            }
        }
        matched
            .iter()
            .chain(unmatched.iter())
            .map(|(id, _)| *id)
            .collect()
    } else {
        candidates.iter().map(|(id, _)| *id).collect()
    };

    serde_json::to_string(&json!({
        "fallbackFileIndexes": fallback_ids,
        "rejectedIndex": rejected
    }))
    .ok()
}

/// Return safe buffer and cache targets for ExoPlayer given preferences and stream type.
pub(crate) fn player_buffer_targets_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<BufferTargetsRequest>(request_json)
        .map_err(|e| CoreError::BadInput {
            context: "player_buffer_targets_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let mobile_data_usage = request.mobile_data_usage.as_deref().unwrap_or("medium");

    // On mobile data, reduce buffers
    let data_factor: f64 = match mobile_data_usage {
        "low" => 0.5,
        "high" => 1.5,
        _ => 1.0,
    };

    let base_forward_ms =
        request.forward_buffer_seconds.unwrap_or(120).clamp(10, 600) as f64 * 1000.0 * data_factor;
    let base_back_ms = request.back_buffer_seconds.unwrap_or(30).clamp(5, 120) as f64 * 1000.0;

    // Torrent streams need smaller buffers to avoid filling the local proxy
    let (forward_ms, back_ms) = if request.is_torrent {
        (base_forward_ms.min(30_000.0), base_back_ms.min(15_000.0))
    } else {
        (base_forward_ms, base_back_ms)
    };

    const UNLIMITED_CACHE_BYTES: i64 = 64_000 * 1_000_000;
    let cache_bytes = match request.cache_size_mb {
        Some(mb) if mb < 0 => UNLIMITED_CACHE_BYTES,
        Some(mb) => mb.clamp(10, 2000) * 1_000_000,
        None => 100 * 1_000_000,
    };

    serde_json::to_string(&json!({
        "forwardBufferMs": forward_ms as i64,
        "backBufferMs": back_ms as i64,
        "cacheSizeBytes": cache_bytes
    }))
    .ok()
}

/// Return the retry/fallback policy given an error code and retry history.
pub(crate) fn player_retry_policy_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<RetryPolicyRequest>(request_json)
        .map_err(|e| CoreError::BadInput {
            context: "player_retry_policy_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let error_code = request.error_code.as_str();
    let retry_count = request.retry_count;

    // Non-retryable errors
    let is_fatal = matches!(
        error_code,
        "no_source"
            | "drm_not_supported"
            | "drm_session_error"
            | "format_unsupported"
            | "missing_profile"
    );

    if is_fatal || retry_count >= 3 {
        return serde_json::to_string(&json!({
            "shouldRetry": false,
            "fallbackAction": if is_fatal { "show_error" } else { "show_error_with_retry_button" },
            "delayMs": 0
        }))
        .ok();
    }

    // Torrent errors get a longer delay
    let (should_retry, delay_ms, fallback_action) = if request.is_torrent {
        match error_code {
            "timeout" | "connection_error" | "buffer_timeout" => {
                (true, 2000u64 * (retry_count as u64 + 1), "retry_stream")
            }
            "torrent_no_file" | "torrent_file_validation_failed" => {
                (true, 1000, "try_fallback_file")
            }
            _ => (false, 0, "show_error"),
        }
    } else {
        match error_code {
            "timeout" | "connection_error" | "io_error" => {
                (true, 1000u64 * (retry_count as u64 + 1), "retry_stream")
            }
            "renderer_error" | "decode_error" => (true, 500, "retry_with_sw_decoder"),
            _ => (false, 0, "show_error"),
        }
    };

    serde_json::to_string(&json!({
        "shouldRetry": should_retry,
        "fallbackAction": fallback_action,
        "delayMs": delay_ms,
        "retryCount": retry_count
    }))
    .ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NextRetrySourceRequest {
    current_stream: Value,
    candidates: Vec<Value>,
    attempted_keys: Vec<String>,
    auto_retry: bool,
    force: bool,
    try_binge_group: bool,
    p2p_enabled: bool,
}

pub(crate) fn next_retry_source_plan_json(request_json: &str) -> Option<String> {
    let request: NextRetrySourceRequest = serde_json::from_str(request_json).ok()?;
    if !request.force && !request.auto_retry {
        return Some(json!({"stream": null, "attemptedKeys": request.attempted_keys}).to_string());
    }
    if request.candidates.len() < 2 {
        return Some(json!({"stream": null, "attemptedKeys": request.attempted_keys}).to_string());
    }
    let current_key = retry_stream_key(&request.current_stream);
    let mut attempted = request.attempted_keys;
    if !current_key.is_empty() && !attempted.contains(&current_key) {
        attempted.push(current_key.clone());
    }
    let start = request
        .candidates
        .iter()
        .position(|candidate| retry_stream_key(candidate) == current_key)
        .unwrap_or(0);
    let binge_group = request
        .try_binge_group
        .then(|| behavior_text(&request.current_stream, "bingeGroup"))
        .flatten();
    for offset in 1..=request.candidates.len() {
        let candidate = &request.candidates[(start + offset) % request.candidates.len()];
        let key = retry_stream_key(candidate);
        if key.is_empty() || attempted.contains(&key) {
            continue;
        }
        if binge_group.is_some() && behavior_text(candidate, "bingeGroup") != binge_group {
            continue;
        }
        if !request.p2p_enabled && stream_is_p2p(candidate) {
            attempted.push(key);
            continue;
        }
        attempted.push(key);
        return Some(json!({"stream": candidate, "attemptedKeys": attempted}).to_string());
    }
    Some(json!({"stream": null, "attemptedKeys": attempted}).to_string())
}

pub(crate) fn playback_close_plan_json(input: &str) -> Option<String> {
    let value: Value = serde_json::from_str(input).ok()?;
    let meta = value.get("meta")?;
    let episode = value.get("episode").filter(|value| !value.is_null());
    let stream = value.get("stream").filter(|value| !value.is_null());
    let next_episode = value.get("nextEpisode").filter(|value| !value.is_null());
    let time_pos = value
        .get("timePos")
        .and_then(Value::as_f64)
        .unwrap_or_default();
    let duration = value
        .get("duration")
        .and_then(Value::as_f64)
        .unwrap_or_default();
    let prefs = value.get("prefs").cloned().unwrap_or_else(|| json!({}));
    let safe_prefs: Value = crate::profile_prefs::profile_safe_prefs_json(&prefs.to_string())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_else(|| json!({"watchedThresholdPercent": 90.0}));
    let threshold = safe_prefs
        .get("watchedThresholdPercent")
        .and_then(Value::as_f64)
        .filter(|value| *value > 0.0)
        .unwrap_or(90.0)
        / 100.0;
    let scrobble_pause = value
        .get("scrobbleTraktPause")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let meaningful = time_pos > 30.0 && duration > 0.0;
    let watched = meaningful && time_pos / duration >= threshold;
    let text_field = |source: Option<&Value>, names: &[&str]| {
        names
            .iter()
            .find_map(|name| source.and_then(|source| source.get(*name)))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let number_field = |source: Option<&Value>, names: &[&str]| {
        names
            .iter()
            .find_map(|name| source.and_then(|source| source.get(*name)))
            .and_then(Value::as_i64)
    };
    let progress = |target: Option<&Value>,
                    position: i64,
                    target_duration: i64,
                    scrobble: bool,
                    include_stream: bool| {
        save_playback_progress_action_value(&SavePlaybackProgressAction {
            profile: None,
            meta: meta.clone(),
            time_offset: position,
            duration: target_duration,
            last_video_id: text_field(target, &["id"]),
            last_stream_index: include_stream
                .then(|| {
                    value
                        .get("streamIndex")
                        .and_then(Value::as_i64)
                        .and_then(|index| i32::try_from(index).ok())
                })
                .flatten(),
            last_episode_name: text_field(target, &["name", "title"]),
            last_episode_season: number_field(target, &["season"]),
            last_episode_number: number_field(target, &["episode", "number"]),
            last_episode_thumbnail: text_field(target, &["thumbnail"]),
            last_stream_url: include_stream
                .then(|| text_field(stream, &["playableUrl", "url"]))
                .flatten(),
            last_stream_title: include_stream
                .then(|| text_field(stream, &["title", "name"]))
                .flatten(),
            last_audio_language: None,
            last_subtitle_language: None,
            scrobble_trakt_pause: Some(scrobble),
        })
    };
    let progress_action = progress(
        episode,
        if meaningful {
            time_pos.floor() as i64
        } else {
            1
        },
        if duration > 0.0 {
            duration.floor() as i64
        } else {
            0
        },
        scrobble_pause,
        true,
    );
    let mark_watched_action = watched.then(|| {
        mark_watched_action_value(&MarkWatchedAction {
            series_id: text_field(Some(meta), &["id"]).unwrap_or_default(),
            video_ids: text_field(episode.or(Some(meta)), &["id"])
                .into_iter()
                .collect(),
            watched: Some(true),
            meta: Some(meta.clone()),
            episodes: episode.map(|episode| {
                vec![json!({
                    "id": text_field(Some(episode), &["id"]),
                    "name": text_field(Some(episode), &["name", "title"]),
                    "season": number_field(Some(episode), &["season"]),
                    "number": number_field(Some(episode), &["episode", "number"]),
                    "thumbnail": text_field(Some(episode), &["thumbnail"]),
                })]
            }),
            profile: None,
        })
    });
    let up_next_action = (watched
        && meta.get("type").and_then(Value::as_str) == Some("series")
        && next_episode.is_some())
    .then(|| {
        progress(
            next_episode,
            UP_NEXT_POSITION_SECONDS,
            UP_NEXT_DURATION_SECONDS,
            false,
            false,
        )
    });
    serde_json::to_string(&json!({"shouldScrobble": meaningful, "progressAction": progress_action, "markWatchedAction": mark_watched_action, "upNextAction": up_next_action, "reloadHome": meaningful})).ok()
}

pub(crate) fn playback_preferences_plan_json(input: &str) -> Option<String> {
    let prefs: Value = serde_json::from_str(input).ok()?;
    let safe: Value = crate::profile_prefs::profile_safe_prefs_json(input)
        .and_then(|json| serde_json::from_str(&json).ok())?;
    serde_json::to_string(&json!({
        "watchedThresholdPercent": safe.get("watchedThresholdPercent"),
        "nextEpisodeThresholdPercent": safe.get("nextEpisodeThresholdPercent"),
        "autoPlayNextEpisode": safe.get("autoPlayNextEpisode"),
        "autoSkipIntro": safe.get("autoSkipIntro"),
        "autoPlayCountdownSecs": prefs.get("autoPlayCountdownSecs").and_then(Value::as_i64).unwrap_or(7).clamp(1, 60),
        "useIntroDb": safe.get("useIntroDb"),
        "useAniSkip": safe.get("useAniSkip"),
        "useAnimeSkip": prefs.get("useAnimeSkip").and_then(Value::as_bool).unwrap_or(true),
        "animeSkipClientId": prefs.get("animeSkipClientId").and_then(Value::as_str).unwrap_or(""),
    })).ok()
}

fn retry_stream_key(stream: &Value) -> String {
    ["url", "playableUrl", "infoHash", "fileIdx", "title", "name"]
        .iter()
        .map(|key| stream.get(key).map(value_key_part).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn stream_shell_plan_json(input: &str) -> Option<String> {
    let stream: Value = serde_json::from_str(input).ok()?;
    let hints = stream.get("behaviorHints");
    let headers = hints
        .and_then(|value| value.get("requestHeaders"))
        .or_else(|| hints.and_then(|value| value.pointer("/proxyHeaders/request")))
        .filter(|value| value.as_object().is_some_and(|map| !map.is_empty()))
        .cloned();
    let source_link = stream
        .get("url")
        .or_else(|| stream.get("infoHash"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let download_link = stream
        .get("playableUrl")
        .or_else(|| stream.get("url"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    serde_json::to_string(&json!({
        "identityKey": retry_stream_key(&stream),
        "isTorrent": stream_is_p2p(&stream)
            || stream.get("url").and_then(Value::as_str).is_some_and(|url| {
                let lower = url.to_ascii_lowercase();
                lower.starts_with("magnet:")
                    || lower.starts_with("stremio://torrent/")
                    || lower.starts_with("infohash:")
            }),
        "requestHeaders": headers,
        "sourceLink": source_link,
        "downloadLink": download_link,
    }))
    .ok()
}

pub(crate) fn order_streams_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let streams = request.get("streams")?.as_array()?;
    let prefs = request.get("prefs")?;
    if prefs
        .get("streamSourceSelectionMode")
        .and_then(Value::as_str)
        != Some("regex")
    {
        return serde_json::to_string(streams).ok();
    }
    let pattern = prefs
        .get("streamSourceRegexPattern")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if pattern.is_empty() {
        return serde_json::to_string(streams).ok();
    }
    let regex = regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .ok()?;
    let mut ordered = streams.clone();
    ordered.sort_by_key(|stream| {
        let text = [
            "name",
            "title",
            "description",
            "url",
            "playableUrl",
            "infoHash",
        ]
        .iter()
        .filter_map(|key| stream.get(*key).and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ");
        std::cmp::Reverse(regex.is_match(&text))
    });
    serde_json::to_string(&ordered).ok()
}

fn value_key_part(value: &Value) -> String {
    value.as_str().map(str::to_string).unwrap_or_else(|| {
        if value.is_null() {
            String::new()
        } else {
            value.to_string().trim_matches('"').to_string()
        }
    })
}

fn behavior_text<'a>(stream: &'a Value, key: &str) -> Option<&'a str> {
    stream
        .get("behaviorHints")
        .and_then(|hints| hints.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn stream_is_p2p(stream: &Value) -> bool {
    stream.get("isTorrent").and_then(Value::as_bool) == Some(true)
        || stream
            .get("infoHash")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
}

/// Build the source sidebar option state: which streams to show and which is selected.
pub fn player_source_sidebar_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<SourceSidebarRequest>(request_json)
        .map_err(|e| CoreError::BadInput {
            context: "player_source_sidebar_plan_json",
            detail: e.to_string(),
        })
        .log_discard()?;
    let current_index = request.current_stream_index.clamp(0, i32::MAX);
    let streams_by_addon: std::collections::BTreeMap<String, Vec<(usize, &Value)>> = request
        .streams
        .iter()
        .enumerate()
        .fold(std::collections::BTreeMap::new(), |mut acc, (i, stream)| {
            let addon_name = stream
                .get("addonName")
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .to_string();
            acc.entry(addon_name).or_default().push((i, stream));
            acc
        });

    let groups: Vec<Value> = streams_by_addon
        .into_iter()
        .map(|(addon_name, streams)| {
            let entries: Vec<Value> = streams
                .iter()
                .map(|(idx, stream)| {
                    json!({
                        "index": idx,
                        "isSelected": *idx == current_index as usize,
                        "title": stream.get("title").cloned().unwrap_or_else(|| json!("")),
                        "name": stream.get("name").cloned().unwrap_or_else(|| json!("")),
                        "quality": stream.get("quality").cloned().unwrap_or(Value::Null)
                    })
                })
                .collect();
            json!({
                "addonName": addon_name,
                "streams": entries,
                "isSelected": entries.iter().any(|e| e["isSelected"].as_bool().unwrap_or(false))
            })
        })
        .collect();

    serde_json::to_string(&json!({
        "groups": groups,
        "currentStreamIndex": current_index,
        "availableAddons": request.available_addons,
        "selectedAddon": request.selected_addon
    }))
    .ok()
}

mod dolby_vision;

pub(crate) use self::dolby_vision::*;
pub(crate) fn can_prefetch_next_episode_json(prefs_json: &str, stream_json: &str) -> bool {
    let prefs: Value = serde_json::from_str(prefs_json).unwrap_or(Value::Null);
    let stream: Value = serde_json::from_str(stream_json).unwrap_or(Value::Null);
    let try_binge = prefs
        .get("tryBingeGroup")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mode = prefs
        .get("streamSourceSelectionMode")
        .and_then(Value::as_str)
        .unwrap_or("manual");
    let has_binge_group = stream
        .get("behaviorHints")
        .and_then(|h| h.get("bingeGroup"))
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty());
    (try_binge && has_binge_group) || mode != "manual"
}

/// Selects the best stream from `streams_json` for the next episode given the
/// current stream and playback preferences. Returns the selected stream as JSON,
/// or `null` if none qualifies.
pub(crate) fn select_next_episode_stream_json(
    streams_json: &str,
    current_stream_json: &str,
    prefs_json: &str,
    next_video_id: &str,
) -> Option<String> {
    let streams: Vec<Value> = serde_json::from_str(streams_json).ok()?;
    if streams.is_empty() {
        return None;
    }
    let current: Value = serde_json::from_str(current_stream_json).ok()?;
    let prefs: Value = serde_json::from_str(prefs_json).unwrap_or(Value::Null);

    let episode_ok = |s: &Value| -> bool {
        let field = |key: &str| -> String {
            s.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let behavior_hints = s.get("behaviorHints");
        let filename = behavior_hints
            .and_then(|h| h.get("filename"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        crate::content_identity::stream_matches_episode(
            next_video_id,
            &[
                field("title"),
                field("name"),
                field("description"),
                filename,
            ],
        )
    };

    let try_binge = prefs
        .get("tryBingeGroup")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mode = prefs
        .get("streamSourceSelectionMode")
        .and_then(Value::as_str)
        .unwrap_or("manual");
    let regex_pat = prefs
        .get("streamSourceRegexPattern")
        .and_then(Value::as_str)
        .unwrap_or("");
    let cur_binge = current
        .get("behaviorHints")
        .and_then(|h| h.get("bingeGroup"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());

    if try_binge {
        if let Some(group) = cur_binge {
            let matched = streams.iter().find(|s| {
                s.get("behaviorHints")
                    .and_then(|h| h.get("bingeGroup"))
                    .and_then(Value::as_str)
                    == Some(group)
                    && episode_ok(s)
            });
            if let Some(s) = matched {
                return serde_json::to_string(s).ok();
            }
        }
    }

    if mode == "regex" && !regex_pat.is_empty() {
        if let Ok(re) = regex::RegexBuilder::new(regex_pat)
            .case_insensitive(true)
            .build()
        {
            let stream_text = |s: &Value| -> String {
                [
                    s.get("name"),
                    s.get("title"),
                    s.get("description"),
                    s.get("url"),
                    s.get("playableUrl"),
                    s.get("infoHash"),
                ]
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
            };
            if let Some(matched) = streams
                .iter()
                .find(|s| re.is_match(&stream_text(s)) && episode_ok(s))
            {
                return serde_json::to_string(matched).ok();
            }
        }
    }

    if let Some(addon_name) = current
        .get("addonName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        if let Some(matched) = streams.iter().find(|stream| {
            stream.get("addonName").and_then(Value::as_str) == Some(addon_name)
                && episode_ok(stream)
        }) {
            return serde_json::to_string(matched).ok();
        }
    }

    streams
        .iter()
        .find(|s| episode_ok(s))
        .and_then(|s| serde_json::to_string(s).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn backend_selection_defaults_to_exoplayer() {
        let result: Value = serde_json::from_str(
            &player_backend_selection_json(r#"{"stream":{"url":"http://example.com/video.mp4"}}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["backend"], "exoplayer");
    }

    #[test]
    fn backend_selection_respects_mpv_user_preference() {
        let result: Value = serde_json::from_str(
            &player_backend_selection_json(
                r#"{"stream":{"url":"http://example.com/video.mp4"},"preferredPlayer":"mpv"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["backend"], "mpv");
        assert_eq!(result["reason"], "user_preference");
    }

    #[test]
    fn torrent_fallback_excludes_rejected_index_and_sorts_by_size() {
        let result: Value = serde_json::from_str(
            &torrent_fallback_file_policy_json(
                r#"{"rejectedIndex":1,"fileStats":[{"id":1,"path":"Big.mkv","length":1000000000},{"id":2,"path":"Small.mkv","length":500000000},{"id":3,"path":"Extras.mkv","length":200000000}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let fallback: Vec<i64> = result["fallbackFileIndexes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        assert!(!fallback.contains(&1), "rejected index must be excluded");
        assert_eq!(fallback[0], 2, "largest remaining file should be first");
    }

    #[test]
    fn buffer_targets_reduces_forward_buffer_for_torrent() {
        let torrent_result: Value = serde_json::from_str(
            &player_buffer_targets_json(
                r#"{"forwardBufferSeconds":120,"backBufferSeconds":30,"isTorrent":true}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let direct_result: Value = serde_json::from_str(
            &player_buffer_targets_json(
                r#"{"forwardBufferSeconds":120,"backBufferSeconds":30,"isTorrent":false}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            torrent_result["forwardBufferMs"].as_i64().unwrap()
                < direct_result["forwardBufferMs"].as_i64().unwrap()
        );
    }

    #[test]
    fn buffer_targets_negative_cache_size_means_unbounded() {
        let result: Value =
            serde_json::from_str(&player_buffer_targets_json(r#"{"cacheSizeMb":-1}"#).unwrap())
                .unwrap();
        assert_eq!(
            result["cacheSizeBytes"].as_i64().unwrap(),
            64_000 * 1_000_000
        );
    }

    #[test]
    fn retry_policy_is_not_retryable_for_no_source() {
        let result: Value = serde_json::from_str(
            &player_retry_policy_json(r#"{"errorCode":"no_source","retryCount":0}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(result["shouldRetry"], false);
    }

    #[test]
    fn retry_policy_retries_connection_errors_with_backoff() {
        let result: Value = serde_json::from_str(
            &player_retry_policy_json(
                r#"{"errorCode":"timeout","retryCount":1,"isTorrent":false}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["shouldRetry"], true);
        assert!(result["delayMs"].as_i64().unwrap() > 0);
    }

    fn plan(json: &str) -> Value {
        serde_json::from_str(&dv_proxy_plan_json(json).unwrap()).unwrap()
    }

    #[test]
    fn dv_proxy_off_mode_returns_none() {
        let p = plan(
            r#"{"stream":{"name":"4K DV HDR","dvProfile":7},"url":"https://cdn.example/movie.mkv","fallbackMode":"off"}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "user_disabled");
    }

    #[test]
    fn dv_proxy_hls_url_defers_to_manifest_rewrite() {
        let p = plan(
            r#"{"stream":{"name":"4K DV","dvProfile":7},"url":"https://cdn.example/index.m3u8","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "manifest_handled");
    }

    #[test]
    fn dv_proxy_dash_url_defers_to_manifest_rewrite() {
        let p = plan(
            r#"{"stream":{"name":"4K DV","dvProfile":7},"url":"https://cdn.example/stream.mpd","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "manifest_handled");
    }

    #[test]
    fn dv_proxy_non_dv_stream_returns_none() {
        let p = plan(
            r#"{"stream":{"name":"1080p HDR AVC"},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "not_dv");
    }

    #[test]
    fn dv_proxy_hw_dv_decoder_skips_proxy() {
        let p = plan(
            r#"{"stream":{"name":"4K DV","dvProfile":7},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":true}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "hw_dv_decoder");
    }

    #[test]
    fn dv_proxy_p5_no_dv_decoder_returns_none() {
        let p = plan(
            r#"{"stream":{"dvProfile":5},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "no_hdr_base_layer");
        assert_eq!(p["profile"], "P5");
    }

    #[test]
    fn dv_proxy_p4_no_dv_decoder_returns_none() {
        let p = plan(
            r#"{"stream":{"dvProfile":4},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "no_hdr_base_layer");
        assert_eq!(p["profile"], "P4");
    }

    #[test]
    fn dv_proxy_p10_compat_0_returns_none() {
        let p = plan(
            r#"{"stream":{"dvProfile":10,"dvCompatId":0},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "p10_compat_id_no_hdr_base");
    }

    #[test]
    fn dv_proxy_p10_compat_2_returns_none() {
        let p = plan(
            r#"{"stream":{"dvProfile":10,"dvCompatId":2},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "none");
    }

    #[test]
    fn dv_proxy_unknown_profile_returns_none() {
        // DV detected but no profile info → safe default is none.
        let p = plan(
            r#"{"stream":{"name":"Dolby Vision"},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "unknown_profile_no_safe_fallback");
    }

    #[test]
    fn dv_proxy_p7_mkv_auto_gives_dvcc_strip_medium_safety() {
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false,"deviceHasDvDisplay":false}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P7");
        assert_eq!(p["compatibility"], "HDR10");
        assert_eq!(p["safety"], "medium");
    }

    #[test]
    fn dv_proxy_p8_1_gives_dvcc_strip_low_safety() {
        let p = plan(
            r#"{"stream":{"dvProfile":8,"dvCompatId":1},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P8.1");
        assert_eq!(p["compatibility"], "HDR10");
        assert_eq!(p["safety"], "low");
    }

    #[test]
    fn dv_proxy_p8_4_fallback_is_hlg_not_hdr10() {
        let p = plan(
            r#"{"stream":{"dvProfile":8,"dvCompatId":4},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P8.4");
        assert_eq!(p["compatibility"], "HLG");
        assert_ne!(p["compatibility"], "HDR10");
    }

    #[test]
    fn dv_proxy_p8_unknown_compat_strips_with_assumed_hdr10() {
        // "DV P8" in name → P8Unknown → strip, medium safety, HDR10_assumed
        let p = plan(
            r#"{"stream":{"name":"DV P8"},"url":"https://debrid.example/file.mkv","fallbackMode":"hdr10","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P8");
        assert_eq!(p["compatibility"], "HDR10_assumed");
        assert_eq!(p["safety"], "medium");
    }

    #[test]
    fn dv_proxy_p10_compat_1_gives_dvcc_strip() {
        let p = plan(
            r#"{"stream":{"dvProfile":10,"dvCompatId":1},"url":"https://cdn.example/movie.mkv","fallbackMode":"auto","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P10_compat1");
        assert_eq!(p["compatibility"], "HDR10");
    }

    #[test]
    fn dv_proxy_p7_raw_hevc_dv8_mode_gives_rpu_convert() {
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/stream.hevc","fallbackMode":"dv8","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "rpu_convert");
        assert_eq!(p["rpuMode"], 2);
        assert_eq!(p["profile"], "P7");
    }

    #[test]
    fn dv_proxy_p7_raw_hevc_auto_dv_display_gives_rpu_convert() {
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/stream.hevc","fallbackMode":"auto","deviceHasDvDecoder":false,"deviceHasDvDisplay":true}"#,
        );
        assert_eq!(p["action"], "rpu_convert");
    }

    #[test]
    fn dv_proxy_rpu_convert_rejected_for_mkv_falls_back_to_dvcc_strip() {
        // dv8 mode + MKV without a DV decoder → falls back to dvcc_strip because
        // rpu_convert needs a DV decoder in the convert_dv81 path, and dv8 mode
        // is annexb-only (rejects non-raw-HEVC containers).
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/movie.mkv","fallbackMode":"dv8","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["reason"], "rpu_convert_rejected_not_annexb");
    }

    #[test]
    fn dv_proxy_rpu_convert_rejected_for_mp4_falls_back_to_dvcc_strip() {
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/movie.mp4","fallbackMode":"dv8","deviceHasDvDecoder":false}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["reason"], "rpu_convert_rejected_not_annexb");
    }

    #[test]
    fn dv_detection_dolby_vision_p8_text_gives_action() {
        // "P8" token → P8Unknown → dvcc_strip
        let p = plan(
            r#"{"stream":{"name":"Dolby Vision P8"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_ne!(p["action"], "none");
        assert_eq!(p["profile"], "P8");
    }

    #[test]
    fn dv_detection_dovi_without_profile_gives_none() {
        // DV detected ("dovi") but no profile info → unknown → none.
        let p = plan(
            r#"{"stream":{"name":"4K DoVi 5.1"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "unknown_profile_no_safe_fallback");
    }

    #[test]
    fn dv_detection_standalone_dv_without_profile_gives_none() {
        // "[DV]" detected but no profile info → none.
        let p = plan(
            r#"{"stream":{"name":"[4K] [DV] [HDR10+]"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "unknown_profile_no_safe_fallback");
    }

    #[test]
    fn dv_detection_dvhe_fourcc_in_name_gives_profile_p7() {
        let p = plan(
            r#"{"stream":{"name":"dvhe.07.06 BDRemux"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_ne!(p["action"], "none");
        assert_eq!(p["profile"], "P7");
    }

    #[test]
    fn dv_detection_dvhe_08_01_in_name_gives_p8_1() {
        let p = plan(
            r#"{"stream":{"name":"dvhe.08.01 Remux"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P8.1");
        assert_eq!(p["safety"], "low");
    }

    #[test]
    fn dv_detection_no_false_positive_from_dvd() {
        let p = plan(
            r#"{"stream":{"name":"DVD Rip 1080p"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "not_dv");
    }

    #[test]
    fn dv_detection_no_false_positive_from_hdvd() {
        let p = plan(
            r#"{"stream":{"name":"HDVD Edition"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "none");
    }

    #[test]
    fn dv_detection_explicit_boolean_flag_with_profile() {
        let p = plan(
            r#"{"stream":{"dv":true,"dvProfile":8,"dvCompatId":1,"name":"4K HDR"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_ne!(p["action"], "none");
        assert_eq!(p["profile"], "P8.1");
    }

    #[test]
    fn dv_detection_filename_without_profile_gives_none() {
        // DV keyword in filename but no profile → safe default is none.
        let p = plan(
            r#"{"stream":{"name":"4K HDR","effectiveFilename":"Movie.2023.UHD.DV.HEVC.mkv"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "unknown_profile_no_safe_fallback");
    }

    #[test]
    fn dv_detection_dvhe_codec_in_filename_gives_profile() {
        let p = plan(
            r#"{"stream":{"effectiveFilename":"Movie.dvhe.07.06.mkv"},"url":"https://cdn.example/f.mkv","fallbackMode":"auto"}"#,
        );
        assert_ne!(p["action"], "none");
        assert_eq!(p["profile"], "P7");
    }

    // These tests mirror real Stremio addon stream objects, covering the full plan output.

    #[test]
    fn sample_p5_dvonly_no_fallback() {
        // P5 is HEVC single-layer with no HDR base. Stripping DVCC would expose
        // a DV-only bitstream to an HDR10 decoder → broken colour. Never rewrite.
        let p = plan(
            r#"{
            "stream": {
                "name": "AETHER | 4K | Dolby Vision | DD+ Atmos",
                "description": "📺 4K | 🎬 dvhe.05.06 | 🔊 DD+ Atmos",
                "dvProfile": 5
            },
            "url": "https://debrid.example/movie.mkv",
            "fallbackMode": "auto",
            "deviceHasDvDecoder": false
        }"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "no_hdr_base_layer");
        assert_eq!(p["profile"], "P5");
        let limitations = p["limitations"].as_array().unwrap();
        assert!(limitations
            .iter()
            .any(|l| l.as_str().unwrap().contains("p4_p5")));
    }

    #[test]
    fn sample_p7_dual_layer_hdr10_fallback() {
        // P7 BL+EL: stripping DVCC reveals the HDR10 base layer. Medium risk —
        // RPU NALs remain in-stream but HEVC decoders ignore them.
        let p = plan(
            r#"{
            "stream": {
                "name": "FLUX | 4K | dvhe.07.06 | Atmos",
                "description": "HDR10 + Dolby Vision P7 BL+EL remux",
                "dvProfile": 7
            },
            "url": "https://realdebrid.com/dl/movie2024.mkv",
            "fallbackMode": "auto",
            "deviceHasDvDecoder": false,
            "deviceHasDvDisplay": false
        }"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P7");
        assert_eq!(p["compatibility"], "HDR10");
        assert_eq!(p["safety"], "medium");
        let limitations = p["limitations"].as_array().unwrap();
        assert!(limitations
            .iter()
            .any(|l| l.as_str().unwrap().contains("does_not_convert_bitstream")));
    }

    #[test]
    fn sample_p8_1_single_layer_low_risk_fallback() {
        // P8.1 has an HDR10-compatible base layer encoded into the single HEVC stream.
        // Stripping DVCC gives clean HDR10 output. Lowest-risk rewrite.
        let p = plan(
            r#"{
            "stream": {
                "name": "HDMUX | 4K | dvhe.08.01 | TrueHD Atmos",
                "dvProfile": 8,
                "dvCompatId": 1
            },
            "url": "https://debrid.example/Movie.2023.2160p.DV.HEVC.mkv",
            "fallbackMode": "auto",
            "deviceHasDvDecoder": false
        }"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P8.1");
        assert_eq!(p["compatibility"], "HDR10");
        assert_eq!(p["safety"], "low");
    }

    #[test]
    fn sample_p8_4_hlg_base_not_hdr10() {
        // P8.4 has an HLG base layer, not HDR10. Rewriting it as HDR10 would
        // produce incorrect colour. The compatibility field must reflect HLG.
        let p = plan(
            r#"{
            "stream": {
                "name": "BBC iPlayer | 4K | Dolby Vision HLG | AAC",
                "dvProfile": 8,
                "dvCompatId": 4
            },
            "url": "https://cdn.example/show_ep01.mkv",
            "fallbackMode": "auto",
            "deviceHasDvDecoder": false
        }"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
        assert_eq!(p["profile"], "P8.4");
        assert_eq!(p["compatibility"], "HLG");
        assert_ne!(
            p["compatibility"], "HDR10",
            "P8.4 has HLG base, must not be labelled HDR10"
        );
        assert_eq!(p["safety"], "medium");
    }

    #[test]
    fn sample_unknown_profile_from_addon_with_only_dv_keyword() {
        // Many addons only set a "Dolby Vision" label without specifying the
        // profile. Without profile info the only safe action is none.
        let p = plan(
            r#"{
            "stream": {
                "name": "4K | Dolby Vision | DD+ Atmos",
                "description": "UHD Remux"
            },
            "url": "https://debrid.example/movie.mkv",
            "fallbackMode": "auto",
            "deviceHasDvDecoder": false
        }"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "unknown_profile_no_safe_fallback");
        let limitations = p["limitations"].as_array().unwrap();
        assert!(limitations
            .iter()
            .any(|l| l.as_str().unwrap().contains("set_dvProfile_field")));
    }

    #[test]
    fn sample_p7_rpu_convert_on_raw_hevc_dv8_mode() {
        // Raw Annex-B HEVC + P7 + dv8 mode → live RPU conversion. The only
        // case where rpu_convert is emitted instead of dvcc_strip.
        let p = plan(
            r#"{
            "stream": {
                "name": "RAW HEVC | 4K | dvhe.07.06",
                "dvProfile": 7
            },
            "url": "https://cdn.example/stream.hevc",
            "fallbackMode": "dv8",
            "deviceHasDvDecoder": false
        }"#,
        );
        assert_eq!(p["action"], "rpu_convert");
        assert_eq!(p["profile"], "P7");
        assert_eq!(p["compatibility"], "DV8");
        assert_eq!(p["rpuMode"], 2);
    }

    #[test]
    fn convert_dv81_p7_mkv_decoder_no_display_returns_rpu_convert() {
        // Decoder present, no DV display: MKV now supported via EBML RPU rewriter.
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/movie.mkv","fallbackMode":"convert_dv81","deviceHasDvDecoder":true,"deviceHasDvDisplay":false}"#,
        );
        assert_eq!(p["action"], "rpu_convert");
        assert_eq!(p["reason"], "p7_rpu_convert_to_dv81");
    }

    #[test]
    fn convert_dv81_p7_mp4_decoder_no_display_returns_rpu_convert() {
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/movie.mp4","fallbackMode":"convert_dv81","deviceHasDvDecoder":true,"deviceHasDvDisplay":false}"#,
        );
        assert_eq!(p["action"], "rpu_convert");
        assert_eq!(p["reason"], "p7_rpu_convert_to_dv81");
    }

    #[test]
    fn convert_dv81_p7_raw_hevc_decoder_no_display_returns_rpu_convert() {
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/movie.hevc","fallbackMode":"convert_dv81","deviceHasDvDecoder":true,"deviceHasDvDisplay":false}"#,
        );
        assert_eq!(p["action"], "rpu_convert");
        assert_eq!(p["reason"], "p7_rpu_convert_to_dv81");
    }

    #[test]
    fn convert_dv81_decoder_and_display_returns_native_passthrough() {
        // Full DV device → native, no proxy needed.
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/movie.mp4","fallbackMode":"convert_dv81","deviceHasDvDecoder":true,"deviceHasDvDisplay":true}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "hw_dv_decoder");
    }

    #[test]
    fn convert_dv81_no_decoder_falls_back_to_dvcc_strip() {
        // No DV decoder → same as Auto: strip to HDR10.
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/movie.mp4","fallbackMode":"convert_dv81","deviceHasDvDecoder":false,"deviceHasDvDisplay":false}"#,
        );
        assert_eq!(p["action"], "dvcc_strip");
    }

    #[test]
    fn convert_dv81_hls_p7_decoder_returns_hls_rpu_convert() {
        // P7 HLS with a DV decoder available: segment-level RPU rewrite
        // (fluxa-streaming-engine's OkHttp interceptor), not the plain
        // manifest passthrough.
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/index.m3u8","fallbackMode":"convert_dv81","deviceHasDvDecoder":true,"deviceHasDvDisplay":false}"#,
        );
        assert_eq!(p["action"], "hls_rpu_convert");
        assert_eq!(p["reason"], "p7_hls_segment_rpu_convert");
    }

    #[test]
    fn convert_dv81_hls_p7_no_decoder_deferred_to_manifest_rewrite() {
        // Without a DV decoder there's nothing to convert into, so HLS
        // still just defers to manifest passthrough.
        let p = plan(
            r#"{"stream":{"dvProfile":7},"url":"https://cdn.example/index.m3u8","fallbackMode":"convert_dv81","deviceHasDvDecoder":false,"deviceHasDvDisplay":false}"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "manifest_handled");
    }

    #[test]
    fn sample_hls_stream_always_deferred_to_manifest_rewrite() {
        // HLS streams are handled by the OkHttp interceptor regardless of profile.
        // The proxy must never be activated for .m3u8 URLs.
        let p = plan(
            r#"{
            "stream": {
                "name": "Apple TV+ | 4K | dvhe.08.01",
                "dvProfile": 8,
                "dvCompatId": 1
            },
            "url": "https://cdn.example/master.m3u8",
            "fallbackMode": "auto",
            "deviceHasDvDecoder": false
        }"#,
        );
        assert_eq!(p["action"], "none");
        assert_eq!(p["reason"], "manifest_handled");
    }
}
