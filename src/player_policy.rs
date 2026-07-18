use crate::core_error::{CoreError, LogAndDiscard};
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
    let threshold = value
        .get("watchedThresholdPercent")
        .and_then(Value::as_f64)
        .filter(|value| *value > 0.0)
        .unwrap_or(90.0)
        / 100.0;
    let meaningful = time_pos > 30.0 && duration > 0.0;
    let watched = meaningful && time_pos / duration >= threshold;
    let field = |source: Option<&Value>, names: &[&str]| {
        names
            .iter()
            .find_map(|name| source.and_then(|source| source.get(*name)))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let progress = |target: Option<&Value>, position: i64, target_duration: i64, scrobble: bool| {
        json!({
            "type": "savePlaybackProgressRequested", "meta": meta, "timeOffset": position, "duration": target_duration,
            "lastVideoId": field(target, &["id"]),
            "lastStreamIndex": if target.is_some() { value.get("streamIndex").cloned().unwrap_or(Value::Null) } else { Value::Null },
            "lastEpisodeName": field(target, &["name", "title"]), "lastEpisodeSeason": field(target, &["season"]),
            "lastEpisodeNumber": field(target, &["episode", "number"]), "lastEpisodeThumbnail": field(target, &["thumbnail"]),
            "lastStreamUrl": if target.is_some() { field(stream, &["playableUrl", "url"]) } else { Value::Null },
            "lastStreamTitle": if target.is_some() { field(stream, &["title", "name"]) } else { Value::Null },
            "lastAudioLanguage": Value::Null, "lastSubtitleLanguage": Value::Null, "scrobbleTraktPause": scrobble,
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
            99_999
        },
        true,
    );
    let mark_watched_action = watched.then(|| json!({
        "type": "markWatchedRequested", "seriesId": meta.get("id").cloned().unwrap_or(Value::Null),
        "videoIds": [field(episode.or(Some(meta)), &["id"])], "watched": true, "meta": meta,
        "episodes": episode.map(|episode| vec![json!({"id": episode.get("id").cloned().unwrap_or(Value::Null), "name": field(Some(episode), &["name", "title"]), "season": episode.get("season").cloned().unwrap_or(Value::Null), "number": field(Some(episode), &["episode", "number"]), "thumbnail": episode.get("thumbnail").cloned().unwrap_or(Value::Null)})]).unwrap_or_default(),
    }));
    let up_next_action = (watched
        && meta.get("type").and_then(Value::as_str) == Some("series")
        && next_episode.is_some())
    .then(|| progress(next_episode, 1, 99_999, false));
    serde_json::to_string(&json!({"shouldScrobble": meaningful, "progressAction": progress_action, "markWatchedAction": mark_watched_action, "upNextAction": up_next_action, "reloadHome": meaningful})).ok()
}

fn retry_stream_key(stream: &Value) -> String {
    ["url", "playableUrl", "infoHash", "fileIdx", "title", "name"]
        .iter()
        .map(|key| stream.get(key).map(value_key_part).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("|")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DvFallbackMode {
    Off,
    #[default]
    Auto,
    Dv8,
    ConvertDv81,
    Hdr10,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DvProxyPlanRequest {
    #[serde(default)]
    stream: Value,
    #[serde(default)]
    url: String,
    #[serde(default)]
    fallback_mode: DvFallbackMode,
    #[serde(default)]
    device_has_dv_decoder: bool,
    #[serde(default)]
    device_has_dv_display: bool,
}

/// Per-stream Dolby Vision profile classification.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DvProfile {
    /// Profile 4 — AVC single-layer, no HDR base layer.
    P4,
    /// Profile 5 — HEVC single-layer, no HDR base layer.
    P5,
    /// Profile 7 — dual-layer BL+EL with HDR10-compatible base.
    P7,
    /// Profile 8, compat_id=1 — single-layer HEVC, HDR10 base. Safest fallback.
    P8Hdr10,
    /// Profile 8, compat_id=4 — single-layer HEVC, HLG base.
    P8Hlg,
    /// Profile 8 with unrecognised compat_id — HDR10 assumed but uncertain.
    P8Unknown,
    /// Profile 10, compat_id=1 — HDR10-compatible base.
    P10Hdr10,
    /// Profile 10, compat_id 0/2/3 — no HDR10 base.
    P10Other,
    /// Profile could not be determined from stream metadata.
    Unknown,
}

impl DvProfile {
    fn label(self) -> &'static str {
        match self {
            DvProfile::P4 => "P4",
            DvProfile::P5 => "P5",
            DvProfile::P7 => "P7",
            DvProfile::P8Hdr10 => "P8.1",
            DvProfile::P8Hlg => "P8.4",
            DvProfile::P8Unknown => "P8",
            DvProfile::P10Hdr10 => "P10_compat1",
            DvProfile::P10Other => "P10_other",
            DvProfile::Unknown => "unknown",
        }
    }
}

/// Returns the recommended DV proxy action for a single stream+URL combination.
///
/// Response fields:
///   action        — "none" | "dvcc_strip" | "rpu_convert"
///   rpuMode       — libdovi convert mode (2 = Profile 8)
///   reason        — machine-readable decision code
///   profile       — detected DV profile ("P7", "P8.1", …, "unknown")
///   compatibility — expected output format ("HDR10", "HLG", "DV8", "DV", "none")
///   safety        — "high" | "medium" | "low" | "none"
///   limitations   — list of known caveats for this action
pub(crate) fn dv_proxy_plan_json(request_json: &str) -> Option<String> {
    let req = serde_json::from_str::<DvProxyPlanRequest>(request_json)
        .map_err(|e| CoreError::BadInput {
            context: "dv_proxy_plan_json",
            detail: e.to_string(),
        })
        .log_discard()?;

    if req.fallback_mode == DvFallbackMode::Off {
        return plan_rich("none", "user_disabled", "unknown", "none", "high", &[]);
    }

    let url_lower = req.url.to_lowercase();
    let is_hls = url_lower.ends_with(".m3u8") || url_lower.contains(".m3u8?");
    let is_dash = url_lower.ends_with(".mpd") || url_lower.contains(".mpd?");

    if !is_dolby_vision_stream(&req.stream, &req.url) {
        return plan_rich("none", "not_dv", "unknown", "none", "high", &[]);
    }

    let native_passthrough = req.device_has_dv_decoder
        && (req.device_has_dv_display || req.fallback_mode != DvFallbackMode::ConvertDv81);
    if native_passthrough {
        return plan_rich("none", "hw_dv_decoder", "unknown", "DV", "high", &[]);
    }

    let profile = detect_dv_profile(&req.stream);
    let container = detect_container(&req.url);

    if is_hls || is_dash {
        if is_hls
            && matches!(profile, DvProfile::P7)
            && req.fallback_mode == DvFallbackMode::ConvertDv81
            && req.device_has_dv_decoder
        {
            return plan_rich(
                "hls_rpu_convert",
                "p7_hls_segment_rpu_convert",
                profile.label(),
                "DV8",
                "medium",
                &[],
            );
        }
        return plan_rich(
            "none",
            "manifest_handled",
            profile.label(),
            "none",
            "high",
            &[],
        );
    }

    // Hard safety gates: profiles with no HDR base layer cannot be safely
    // rewritten — stripping DVCC would expose a DV-only bitstream to an
    // HDR10 decoder, producing corrupted colour.
    match profile {
        DvProfile::P4 | DvProfile::P5 => {
            return plan_rich(
                "none",
                "no_hdr_base_layer",
                profile.label(),
                "none",
                "none",
                &["p4_p5_no_hdr_fallback_possible"],
            );
        }
        DvProfile::P10Other => {
            return plan_rich(
                "none",
                "p10_compat_id_no_hdr_base",
                profile.label(),
                "none",
                "none",
                &["only_p10_compat_id_1_has_hdr10_base"],
            );
        }
        // Unknown profile: do nothing rather than guess and corrupt playback.
        DvProfile::Unknown => {
            return plan_rich(
                "none",
                "unknown_profile_no_safe_fallback",
                "unknown",
                "none",
                "none",
                &["set_dvProfile_field_or_codec_string_for_safe_rewrite"],
            );
        }
        _ => {}
    }

    let mode = req.fallback_mode;

    let (action, compat, safety, reason, limitations): (&str, &str, &str, &str, Vec<&str>) =
        match profile {
            DvProfile::P7 => match (mode, &container) {
                // convert_dv81 + DV decoder (no display): RPU conversion for Annex-B + fMP4.
                // Without a DV decoder, conversion would produce DV8.1 that nothing can decode;
                // fall through to dvcc_strip (handled by the catch-all arm below).
                (DvFallbackMode::ConvertDv81, _) if req.device_has_dv_decoder => (
                    "rpu_convert",
                    "DV8",
                    "medium",
                    "p7_rpu_convert_to_dv81",
                    vec![],
                ),
                (DvFallbackMode::Dv8, DvContainer::RawHevc) => (
                    "rpu_convert",
                    "DV8",
                    "medium",
                    "p7_rpu_convert_to_dv8_annexb",
                    vec!["annexb_only"],
                ),
                // Auto mode + DV-capable display: keep DV via RPU conversion.
                (DvFallbackMode::Auto, DvContainer::RawHevc) if req.device_has_dv_display => (
                    "rpu_convert",
                    "DV8",
                    "medium",
                    "p7_rpu_convert_auto_dv_display_annexb",
                    vec!["annexb_only"],
                ),
                // dv8 mode requested but container is not Annex-B.
                (DvFallbackMode::Dv8, _) => (
                    "dvcc_strip",
                    "HDR10",
                    "medium",
                    "rpu_convert_rejected_not_annexb",
                    vec![
                        "rpu_convert_requires_annexb_hevc",
                        "container_is_not_raw_hevc_fallback_to_dvcc_strip",
                        "header_only_patch",
                        "does_not_transcode",
                        "does_not_remove_rpu_nals",
                    ],
                ),
                _ => (
                    "dvcc_strip",
                    "HDR10",
                    "medium",
                    "p7_dvcc_strip_hdr10_base",
                    vec![
                        "does_not_convert_bitstream",
                        "rpu_nals_remain_in_stream_ignored",
                        "header_only_patch",
                        "does_not_transcode",
                        "does_not_remove_rpu_nals",
                    ],
                ),
            },
            DvProfile::P8Hdr10 => (
                "dvcc_strip",
                "HDR10",
                "low",
                "p8_1_hdr10_compat_base",
                vec![
                    "single_layer_hdr10_base_fully_compatible",
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
            DvProfile::P8Hlg => (
                "dvcc_strip",
                "HLG",
                "medium",
                "p8_4_hlg_compat_base",
                vec![
                    "hlg_base_not_hdr10_color_rendering_may_differ",
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
            DvProfile::P8Unknown => (
                "dvcc_strip",
                "HDR10_assumed",
                "medium",
                "p8_compat_id_unknown_hdr10_assumed",
                vec![
                    "compat_id_unknown_hdr10_base_assumed",
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
            DvProfile::P10Hdr10 => (
                "dvcc_strip",
                "HDR10",
                "medium",
                "p10_compat_id_1_hdr10_base",
                vec![
                    "does_not_convert_bitstream",
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
            _ => (
                "dvcc_strip",
                "HDR10_assumed",
                "medium",
                "unknown_profile_dvcc_strip_fallback",
                vec![
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
        };

    plan_rich(
        action,
        reason,
        profile.label(),
        compat,
        safety,
        &limitations,
    )
}

fn plan_rich(
    action: &str,
    reason: &str,
    profile: &str,
    compatibility: &str,
    safety: &str,
    limitations: &[&str],
) -> Option<String> {
    serde_json::to_string(&json!({
        "action": action,
        "rpuMode": 2u8,
        "reason": reason,
        "profile": profile,
        "compatibility": compatibility,
        "safety": safety,
        "limitations": limitations,
    }))
    .ok()
}

/// Derive the DV profile from stream metadata, codec strings, and text hints.
fn detect_dv_profile(stream: &Value) -> DvProfile {
    // 1. Explicit integer fields set by the addon.
    let profile_num = stream
        .get("dvProfile")
        .or_else(|| stream.get("dv_profile"))
        .and_then(Value::as_i64);
    let compat_id = stream
        .get("dvCompatId")
        .or_else(|| stream.get("dvCompatibility"))
        .and_then(Value::as_i64);
    if let Some(p) = profile_num {
        return profile_from_nums(p, compat_id);
    }

    // 2. ISO-BMFF / HLS codec string: "dvhe.07.06", "dvh1.08.01", …
    let codecs = stream.get("codecs").and_then(Value::as_str).unwrap_or("");
    if let Some(p) = parse_dv_codec_string(codecs) {
        return p;
    }

    // 3. Codec token embedded in freetext fields (e.g., "dvhe.07.06 BDRemux").
    let name = stream.get("name").and_then(Value::as_str).unwrap_or("");
    let desc = stream
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let filename = stream
        .get("effectiveFilename")
        .or_else(|| stream.get("filename"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let text = format!("{} {} {}", name, desc, filename);
    if let Some(p) = parse_dv_codec_string(&text) {
        return p;
    }

    // 4. Short profile tokens: "P8.1", "P7", "P8", …
    parse_dv_profile_text(&text).unwrap_or(DvProfile::Unknown)
}

fn profile_from_nums(profile: i64, compat_id: Option<i64>) -> DvProfile {
    match profile {
        4 => DvProfile::P4,
        5 => DvProfile::P5,
        7 => DvProfile::P7,
        8 => match compat_id {
            Some(1) => DvProfile::P8Hdr10,
            Some(4) => DvProfile::P8Hlg,
            _ => DvProfile::P8Unknown,
        },
        10 => match compat_id {
            Some(1) => DvProfile::P10Hdr10,
            _ => DvProfile::P10Other,
        },
        _ => DvProfile::Unknown,
    }
}

/// Parse a DV fourcc codec string such as "dvhe.07.06" → P7.
fn parse_dv_codec_string(text: &str) -> Option<DvProfile> {
    let lower = text.to_lowercase();
    for prefix in &["dvhe.", "dvh1.", "dva1.", "dvav."] {
        if let Some(pos) = lower.find(prefix) {
            let after = &text[pos + prefix.len()..];
            let mut parts = after.splitn(3, '.');
            // Take only the leading digits from each field (e.g. "08" from "08.01 Remux").
            let profile: i64 = leading_digits(parts.next()?)?.parse().ok()?;
            let compat: Option<i64> = parts
                .next()
                .and_then(leading_digits)
                .and_then(|s| s.parse().ok());
            return Some(profile_from_nums(profile, compat));
        }
    }
    None
}

fn leading_digits(s: &str) -> Option<&str> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some(&s[..end])
    }
}

/// Recognise short profile tokens ("P8.1", "P7", "P8") in freetext.
fn parse_dv_profile_text(text: &str) -> Option<DvProfile> {
    // Ordered so longer patterns match before their shorter prefixes.
    let patterns: &[(&str, DvProfile)] = &[
        ("P8.1", DvProfile::P8Hdr10),
        ("P8.4", DvProfile::P8Hlg),
        ("P7", DvProfile::P7),
        ("P8", DvProfile::P8Unknown),
        ("P10", DvProfile::P10Other),
        ("P5", DvProfile::P5),
        ("P4", DvProfile::P4),
    ];
    for (pat, profile) in patterns {
        if contains_word(text, pat) {
            return Some(*profile);
        }
    }
    None
}

/// True when `word` appears in `text` surrounded by non-alphanumeric (or absent) bytes.
fn contains_word(text: &str, word: &str) -> bool {
    let tb = text.as_bytes();
    let wb = word.as_bytes();
    let wlen = wb.len();
    if tb.len() < wlen {
        return false;
    }
    for i in 0..=(tb.len() - wlen) {
        if &tb[i..i + wlen] == wb {
            let before_ok = i == 0 || !tb[i - 1].is_ascii_alphanumeric();
            let after_ok = i + wlen >= tb.len() || !tb[i + wlen].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

/// Returns true when the stream or URL is identifiable as Dolby Vision content.
fn is_dolby_vision_stream(stream: &Value, url: &str) -> bool {
    if stream.get("dv").and_then(Value::as_bool).unwrap_or(false)
        || stream
            .get("dolbyVision")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || stream.get("dvProfile").and_then(Value::as_i64).is_some()
    {
        return true;
    }

    let name = stream.get("name").and_then(Value::as_str).unwrap_or("");
    let desc = stream
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let filename = stream
        .get("effectiveFilename")
        .or_else(|| stream.get("filename"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let lower = format!("{} {} {} {}", name, desc, filename, url).to_lowercase();

    if lower.contains("dvhe")
        || lower.contains("dvh1")
        || lower.contains("dva1")
        || lower.contains("dvav")
        || lower.contains("dolby vision")
        || lower.contains("dolby-vision")
        || lower.contains("dovi")
    {
        return true;
    }

    // "DV" as a standalone token (case-sensitive — avoids "DVD", "HDVD", etc.).
    is_standalone_dv_token(&format!("{} {} {} {}", name, desc, filename, url))
}

fn is_standalone_dv_token(text: &str) -> bool {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 1 < len {
        if bytes[i] == b'D' && bytes[i + 1] == b'V' {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphabetic();
            let after_ok = i + 2 >= len || !bytes[i + 2].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

enum DvContainer {
    Mkv,
    Mp4,
    RawHevc,
    Unknown,
}

fn detect_container(url: &str) -> DvContainer {
    let path = url.split('?').next().unwrap_or(url).to_lowercase();
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "mkv" | "mk3d" | "mka" | "mks" | "webm" => DvContainer::Mkv,
        "mp4" | "m4v" | "m4a" | "mov" => DvContainer::Mp4,
        "hevc" | "h265" | "265" => DvContainer::RawHevc,
        _ => DvContainer::Unknown,
    }
}

fn episode_path_matches_id(path: &str, video_id: &str) -> bool {
    let parts: Vec<&str> = video_id.split(':').collect();
    if parts.len() < 3 {
        return false;
    }
    let season = parts[1].parse::<i32>().unwrap_or(0);
    let episode = parts[2].parse::<i32>().unwrap_or(0);
    if season == 0 || episode == 0 {
        return false;
    }
    let path_lower = path.to_lowercase();
    let pattern_s_e = format!("s{:02}e{:02}", season, episode);
    let pattern_sx_ex = format!("{}x{:02}", season, episode);
    let pattern_ep = format!("e{:02}", episode);
    path_lower.contains(&pattern_s_e)
        || path_lower.contains(&pattern_sx_ex)
        || path_lower.contains(&pattern_ep)
}

/// Returns true when the player should attempt to pre-fetch the next episode's
/// stream list. Uses the same binge-group / auto-selection rules as the desktop.
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
