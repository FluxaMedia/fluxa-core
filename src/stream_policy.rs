use crate::content_identity::stream_matches_episode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

const VIDEO_FILE_EXTENSIONS: [&str; 7] = [".mkv", ".mp4", ".avi", ".webm", ".m4v", ".mov", ".ts"];

// Any value the platform sends that isn't "regex"/"first" behaves as "manual" —
// this mirrors the pre-existing catch-all match arm, just moved to the one
// place the raw string gets parsed instead of being re-compared everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SourceSelectionMode {
    Regex,
    First,
    #[default]
    Manual,
}

impl From<&str> for SourceSelectionMode {
    fn from(value: &str) -> Self {
        match value {
            "regex" => SourceSelectionMode::Regex,
            "first" => SourceSelectionMode::First,
            _ => SourceSelectionMode::Manual,
        }
    }
}

pub(crate) fn stream_behavior_text<'a>(stream: &'a Value, key: &str) -> Option<&'a str> {
    stream
        .get("behaviorHints")
        .and_then(|hints| hints.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

pub(crate) fn stream_text<'a>(stream: &'a Value, key: &str) -> Option<&'a str> {
    stream
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

pub(crate) fn stream_number(stream: &Value, key: &str) -> Option<i64> {
    stream.get(key).and_then(Value::as_i64).or_else(|| {
        stream
            .get("behaviorHints")
            .and_then(|hints| hints.get(key))
            .and_then(Value::as_i64)
    })
}

pub(crate) fn stream_playable_url(stream: &Value) -> Option<String> {
    if let Some(url) = stream_text(stream, "url") {
        return Some(url.to_string());
    }
    if let Some(yt_id) = stream_text(stream, "ytId") {
        return Some(format!("https://www.youtube.com/watch?v={yt_id}"));
    }
    if let Some(yt_id) = stream_text(stream, "yt_ID") {
        return Some(format!("https://www.youtube.com/watch?v={yt_id}"));
    }
    if let Some(external_url) = stream_text(stream, "externalUrl") {
        return Some(external_url.to_string());
    }
    let info_hash = stream_text(stream, "infoHash")?;
    match stream.get("fileIdx").and_then(Value::as_i64) {
        Some(file_idx) => Some(format!("stremio://torrent/{info_hash}/{file_idx}")),
        None => Some(format!("stremio://torrent/{info_hash}")),
    }
}

pub(crate) fn percent_decode_component(value: &str) -> String {
    let mut bytes = Vec::with_capacity(value.len());
    let raw = value.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        // Decode the two hex digits as raw bytes rather than slicing `value` —
        // a `%` next to a multi-byte UTF-8 character can put the slice bound
        // mid-character, which panics; byte-at-a-time reads can't.
        if raw[index] == b'%' && index + 2 < raw.len() {
            let hi = (raw[index + 1] as char).to_digit(16);
            let lo = (raw[index + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                bytes.push((hi * 16 + lo) as u8);
                index += 3;
                continue;
            }
        }
        bytes.push(raw[index]);
        index += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(crate) fn stream_effective_filename(
    stream: &Value,
    playable_url: Option<&str>,
) -> Option<String> {
    if let Some(filename) = stream_text(stream, "filename") {
        return Some(filename.to_string());
    }
    if let Some(filename) = stream_behavior_text(stream, "filename") {
        return Some(filename.to_string());
    }
    let url = stream_text(stream, "url").or(playable_url)?;
    let path = url
        .split('?')
        .next()
        .unwrap_or(url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    if path.is_empty() {
        None
    } else {
        Some(percent_decode_component(path))
    }
}

pub(crate) fn form_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'*') {
            encoded.push(byte as char);
        } else if byte == b' ' {
            encoded.push('+');
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

pub(crate) fn is_torrent_playback_url(value: &str) -> bool {
    value.starts_with("stremio://torrent/")
        || value.starts_with("magnet:")
        || value.starts_with("infohash:")
}

pub(crate) fn stream_is_likely_player_compatible(
    _stream: &Value,
    playable_url: Option<&str>,
    _effective_filename: Option<&str>,
) -> bool {
    let Some(candidate) = playable_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let normalized = candidate.to_ascii_lowercase();
    if is_torrent_playback_url(&normalized) {
        return true;
    }
    if !normalized.starts_with("http://") && !normalized.starts_with("https://") {
        return false;
    }
    true
}

pub(crate) fn stream_playback_info_json(stream_json: &str) -> Option<String> {
    let stream = serde_json::from_str::<Value>(stream_json).ok()?;
    let playable_url = stream_playable_url(&stream);
    let effective_video_hash = stream_text(&stream, "videoHash")
        .or_else(|| stream_behavior_text(&stream, "videoHash"))
        .map(str::to_string);
    let effective_video_size =
        stream_number(&stream, "videoSize").or_else(|| stream_number(&stream, "size"));
    let effective_filename = stream_effective_filename(&stream, playable_url.as_deref());
    let subtitle_parts = [
        effective_video_hash
            .as_ref()
            .map(|value| ("videoHash", value.clone())),
        effective_video_size.map(|value| ("videoSize", value.to_string())),
        effective_filename
            .as_ref()
            .map(|value| ("filename", value.clone())),
    ]
    .into_iter()
    .flatten()
    .map(|(key, value)| format!("{}={}", form_encode(key), form_encode(&value)))
    .collect::<Vec<_>>();
    let is_torrent = playable_url
        .as_deref()
        .map(is_torrent_playback_url)
        .unwrap_or(false);
    let is_compatible = stream_is_likely_player_compatible(
        &stream,
        playable_url.as_deref(),
        effective_filename.as_deref(),
    );
    serde_json::to_string(&json!({
        "playableUrl": playable_url,
        "effectiveVideoHash": effective_video_hash,
        "effectiveVideoSize": effective_video_size,
        "effectiveFilename": effective_filename,
        "subtitleExtraArgs": subtitle_parts.join("&"),
        "isTorrentPlaybackUrl": is_torrent,
        "isLikelyPlayerCompatible": is_compatible
    }))
    .ok()
}

pub(crate) fn stream_request_headers_json(headers_json: &str) -> Option<String> {
    let headers = serde_json::from_str::<HashMap<String, String>>(headers_json).ok()?;
    let clean = headers
        .into_iter()
        .filter(|(key, value)| !key.trim().is_empty() && !value.trim().is_empty())
        .collect::<HashMap<_, _>>();
    serde_json::to_string(&clean).ok()
}

pub(crate) fn stream_request_referer(_url: &str) -> Option<String> {
    None
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TorrentRuntimeRequest {
    link: String,
    title: String,
    requested_file_idx: Option<i32>,
    preferred_filename: Option<String>,
    sources: Vec<String>,
    file_stats: Vec<TorrentFileStat>,
    rejected_index: Option<i32>,
    base_url: String,
    play: bool,
    stat: bool,
}

#[derive(Clone, serde::Deserialize)]
pub(crate) struct TorrentFileStat {
    id: i32,
    path: String,
    length: i64,
}

pub(crate) fn is_bare_info_hash(value: &str) -> bool {
    let length = value.len();
    matches!(length, 32 | 40 | 64) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub(crate) fn normalize_torrent_link(link: &str, sources: &[String]) -> String {
    let trimmed = link.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("stremio://torrent/") {
        let rest = &trimmed["stremio://torrent/".len()..];
        let hash = rest.split('/').next().unwrap_or("").trim();
        if hash.is_empty() {
            return trimmed.to_string();
        }
        return build_magnet(hash, sources);
    }
    if lower.starts_with("infohash:") {
        return build_magnet(
            trimmed
                .split_once(':')
                .map(|(_, value)| value)
                .unwrap_or(""),
            sources,
        );
    }
    if is_bare_info_hash(trimmed) {
        return build_magnet(trimmed, sources);
    }
    trimmed.to_string()
}

// Popular fallback trackers always added to magnets so a bare info_hash (no
// addon-provided sources) doesn't have to round-trip DHT for peer discovery.
// Kept short — duplicates from `sources` are filtered out below.
const FALLBACK_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://open.demonii.com:1337/announce",
];

pub(crate) fn build_magnet(hash: &str, sources: &[String]) -> String {
    let mut trackers = Vec::new();
    for source in sources {
        let tracker = source.strip_prefix("tracker:").unwrap_or(source).trim();
        if (tracker.starts_with("udp://")
            || tracker.starts_with("http://")
            || tracker.starts_with("https://"))
            && !trackers.contains(&tracker.to_string())
        {
            trackers.push(tracker.to_string());
        }
    }
    for fallback in FALLBACK_TRACKERS {
        let tracker = fallback.to_string();
        if !trackers.contains(&tracker) {
            trackers.push(tracker);
        }
    }
    let tracker_query = trackers
        .iter()
        .map(|tracker| format!("&tr={}", form_encode(tracker)))
        .collect::<String>();
    format!(
        "magnet:?xt=urn:btih:{}{}",
        hash.to_ascii_lowercase(),
        tracker_query
    )
}

pub(crate) fn stream_magnet_link(stream: &Value) -> Option<String> {
    let sources = stream
        .get("sources")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(hash) = stream_text(stream, "infoHash") {
        return Some(build_magnet(hash, &sources));
    }
    let playable = stream_playable_url(stream)?;
    if playable.to_ascii_lowercase().starts_with("magnet:") {
        return Some(playable);
    }
    if is_torrent_playback_url(&playable) {
        return Some(normalize_torrent_link(&playable, &sources));
    }
    None
}

pub(crate) fn stream_magnet_link_json(stream_json: &str) -> Option<String> {
    let stream: Value = serde_json::from_str(stream_json).ok()?;
    stream_magnet_link(&stream)
}

pub(crate) fn normalize_torrent_file_name(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .replace('\\', "/")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub(crate) fn is_likely_video_file(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    VIDEO_FILE_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
}

pub(crate) fn resolve_torrent_file_index(
    _title: &str,
    requested_file_idx: Option<i32>,
    preferred_filename: Option<&str>,
    file_stats: &[TorrentFileStat],
) -> (Option<i32>, Option<String>) {
    // addon-provided fileIdx is authoritative — use it directly
    if let Some(idx) = requested_file_idx {
        return (Some(idx), Some("requested".to_string()));
    }

    if file_stats.is_empty() {
        return (None, None);
    }

    if let Some(preferred) = preferred_filename
        .map(normalize_torrent_file_name)
        .filter(|value| !value.is_empty())
    {
        if let Some(stat) = file_stats.iter().find(|stat| {
            let path = normalize_torrent_file_name(&stat.path);
            path == preferred
                || path.ends_with(&format!("/{preferred}"))
                || path.rsplit('/').next() == Some(preferred.as_str())
        }) {
            return (Some(stat.id), Some("filename".to_string()));
        }
    }

    file_stats
        .iter()
        .filter(|stat| is_likely_video_file(&stat.path))
        .max_by_key(|stat| stat.length)
        .map(|stat| (Some(stat.id), Some("largest-video".to_string())))
        .unwrap_or((None, None))
}

pub(crate) fn torrent_fallback_file_indexes(
    _title: &str,
    rejected_index: Option<i32>,
    file_stats: &[TorrentFileStat],
) -> Vec<i32> {
    let mut videos: Vec<&TorrentFileStat> = file_stats
        .iter()
        .filter(|stat| is_likely_video_file(&stat.path))
        .collect();
    videos.sort_by_key(|stat| std::cmp::Reverse(stat.length));
    videos
        .into_iter()
        .filter(|stat| Some(stat.id) != rejected_index)
        .map(|stat| stat.id)
        .collect()
}

pub(crate) fn query_encode(value: &str) -> String {
    form_encode(value).replace('+', "%20")
}

pub(crate) fn build_torrent_stream_url(
    base_url: &str,
    link: &str,
    title: &str,
    file_idx: Option<i32>,
    play: bool,
    stat: bool,
) -> String {
    let base = format!("{}/stream/fname", base_url.trim_end_matches('/'));
    let mut query = format!("link={}", query_encode(link));
    if let Some(index) = file_idx {
        query.push_str(&format!("&index={index}"));
    }
    if play {
        query.push_str("&play");
    }
    if stat {
        query.push_str("&stat");
    }
    query.push_str(&format!("&title={}", query_encode(title)));
    format!("{base}?{query}")
}

pub(crate) fn torrent_runtime_info_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<TorrentRuntimeRequest>(request_json).ok()?;
    let normalized_link = normalize_torrent_link(&request.link, &request.sources);
    let (selected_file_idx, selected_reason) = resolve_torrent_file_index(
        &request.title,
        request.requested_file_idx,
        request.preferred_filename.as_deref(),
        &request.file_stats,
    );
    let fallback_file_indexes =
        torrent_fallback_file_indexes(&request.title, request.rejected_index, &request.file_stats);
    let stream_url = build_torrent_stream_url(
        &request.base_url,
        &normalized_link,
        &request.title,
        selected_file_idx,
        request.play,
        request.stat,
    );
    serde_json::to_string(&json!({
        "normalizedLink": normalized_link,
        "selectedFileIdx": selected_file_idx,
        "selectedReason": selected_reason,
        "fallbackFileIndexes": fallback_file_indexes,
        "streamUrl": stream_url
    }))
    .ok()
}

pub(crate) fn torrent_buffer_progress(status: &Value) -> i32 {
    let stat = status.get("stat").and_then(Value::as_i64).unwrap_or(0);
    let preload = status.get("preload").and_then(Value::as_i64).unwrap_or(0);
    let loaded_size = status
        .get("loaded_size")
        .or_else(|| status.get("loadedSize"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let preload_size = status
        .get("preload_size")
        .or_else(|| status.get("preloadSize"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let progress = status
        .get("progress")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let value = if stat >= 3 {
        100
    } else if preload > 0 {
        preload as i32
    } else if preload_size > 0 {
        ((loaded_size as f64 / preload_size as f64) * 100.0) as i32
    } else if loaded_size > 0 {
        ((loaded_size as f64 / (512.0 * 1024.0)) * 100.0) as i32
    } else {
        progress as i32
    };
    value.clamp(0, 100)
}

pub(crate) fn torrent_is_playable_enough(status: &Value) -> bool {
    let stat = status.get("stat").and_then(Value::as_i64).unwrap_or(0);
    let preload = status.get("preload").and_then(Value::as_i64).unwrap_or(0);
    let loaded_size = status
        .get("loaded_size")
        .or_else(|| status.get("loadedSize"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let preload_size = status
        .get("preload_size")
        .or_else(|| status.get("preloadSize"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let target = if preload_size > 0 {
        preload_size.min(4 * 1024 * 1024)
    } else {
        512 * 1024
    };
    stat >= 3 || preload >= 100 || loaded_size >= target
}

pub(crate) fn torrent_status_key(status: &Value) -> &'static str {
    match status.get("stat").and_then(Value::as_i64).unwrap_or(0) {
        1 => "player.torrent_status.preloading",
        2 => "player.torrent_status.downloading",
        3 => "player.torrent_status.ready",
        _ => "player.torrent_status.loading_metadata",
    }
}

pub(crate) fn torrent_status_info_json(status_json: &str) -> Option<String> {
    let status = serde_json::from_str::<Value>(status_json).ok()?;
    serde_json::to_string(&json!({
        "bufferProgress": torrent_buffer_progress(&status),
        "isPlayableEnough": torrent_is_playable_enough(&status),
        "statusKey": torrent_status_key(&status)
    }))
    .ok()
}

pub(crate) fn normalize_language(value: &str) -> String {
    value.to_lowercase()
}

pub(crate) fn normalize_language_preference(value: &str) -> String {
    normalize_language(value)
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_string()
}

pub(crate) fn resolve_preferred_audio_language(
    last_audio_language: Option<&str>,
    preferred_audio_language: Option<&str>,
    original_language: Option<&str>,
) -> String {
    if let Some(memory) = last_audio_language
        .map(normalize_language)
        .filter(|value| !value.trim().is_empty())
    {
        return memory;
    }
    let Some(preferred) = preferred_audio_language
        .map(normalize_language)
        .filter(|value| value != "none")
    else {
        return String::new();
    };
    if preferred != "en" {
        return preferred;
    }
    if original_language.map(normalize_language).as_deref() == Some("ja") {
        "ja".to_string()
    } else {
        preferred
    }
}

// The preference string changes once per settings edit, not per stream, so a
// one-entry cache avoids recompiling the same word-boundary regex on every
// track in a per-stream ranking loop.
fn word_boundary_regex_for(normalized_preference: &str) -> Option<regex::Regex> {
    static CACHE: Mutex<Option<(String, regex::Regex)>> = Mutex::new(None);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((cached_preference, regex)) = cache.as_ref() {
        if cached_preference == normalized_preference {
            return Some(regex.clone());
        }
    }
    let regex =
        regex::Regex::new(&format!(r"\b{}\b", regex::escape(normalized_preference))).ok()?;
    *cache = Some((normalized_preference.to_string(), regex.clone()));
    Some(regex)
}

pub(crate) fn subtitle_language_alias_matches(label: &str, normalized_preference: &str) -> bool {
    match normalized_preference {
        "tr" => ["turkish", "turkce", "turk", "altyazi", "altyazı"]
            .iter()
            .any(|alias| label.contains(alias)),
        "en" => ["english", "eng"].iter().any(|alias| label.contains(alias)),
        "ja" => ["japanese", "jpn"]
            .iter()
            .any(|alias| label.contains(alias)),
        _ => false,
    }
}

pub(crate) fn subtitle_language_matches(
    label: &str,
    language: Option<&str>,
    preferred_language: &str,
) -> bool {
    let normalized_preference = normalize_language_preference(preferred_language);
    let word_regex = word_boundary_regex_for(&normalized_preference);
    subtitle_language_matches_precompiled(
        label,
        language,
        &normalized_preference,
        word_regex.as_ref(),
    )
}

fn subtitle_language_matches_precompiled(
    label: &str,
    language: Option<&str>,
    normalized_preference: &str,
    word_regex: Option<&regex::Regex>,
) -> bool {
    if normalized_preference.is_empty() {
        return false;
    }
    let language = normalize_language(language.unwrap_or(""));
    if language.starts_with(normalized_preference) {
        return true;
    }
    let label = normalize_language(label);
    word_regex.is_some_and(|regex| regex.is_match(&label))
        || subtitle_language_alias_matches(&label, normalized_preference)
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubtitleSelectionTrack {
    id: Option<String>,
    label: String,
    language: Option<String>,
}

fn find_preferred_subtitle_index_in_tracks(
    tracks: &[SubtitleSelectionTrack],
    last_subtitle_language: Option<&str>,
    preferred_subtitle_language: Option<&str>,
    secondary_subtitle_language: Option<&str>,
) -> i32 {
    let primary = last_subtitle_language
        .filter(|value| !value.is_empty() && *value != "__off__")
        .or_else(|| preferred_subtitle_language.filter(|value| *value != "none"));
    if let Some(preferred) = primary {
        let norm = normalize_language_preference(preferred);
        let word_regex = word_boundary_regex_for(&norm);
        if let Some(index) = tracks.iter().position(|track| {
            subtitle_language_matches_precompiled(
                &track.label,
                track.language.as_deref(),
                &norm,
                word_regex.as_ref(),
            )
        }) {
            return index as i32;
        }
    }
    if let Some(secondary) = secondary_subtitle_language.filter(|value| *value != "none") {
        let norm = normalize_language_preference(secondary);
        let word_regex = word_boundary_regex_for(&norm);
        if let Some(index) = tracks.iter().position(|track| {
            subtitle_language_matches_precompiled(
                &track.label,
                track.language.as_deref(),
                &norm,
                word_regex.as_ref(),
            )
        }) {
            return index as i32;
        }
    }
    -1
}

pub(crate) fn find_preferred_subtitle_index(
    tracks_json: &str,
    last_subtitle_language: Option<&str>,
    preferred_subtitle_language: Option<&str>,
    secondary_subtitle_language: Option<&str>,
) -> i32 {
    let Ok(tracks) = serde_json::from_str::<Vec<SubtitleSelectionTrack>>(tracks_json) else {
        return -1;
    };
    find_preferred_subtitle_index_in_tracks(
        &tracks,
        last_subtitle_language,
        preferred_subtitle_language,
        secondary_subtitle_language,
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerTrackStateRequest {
    #[serde(default)]
    available_subtitles: Vec<SubtitleSelectionTrack>,
    last_audio_language: Option<String>,
    preferred_audio_language: Option<String>,
    original_language: Option<String>,
    last_subtitle_language: Option<String>,
    preferred_subtitle_language: Option<String>,
    secondary_subtitle_language: Option<String>,
}

pub(crate) fn player_track_state_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<PlayerTrackStateRequest>(request_json).ok()?;
    let preferred_audio_language = resolve_preferred_audio_language(
        request
            .last_audio_language
            .as_deref()
            .filter(|value| !value.is_empty()),
        request
            .preferred_audio_language
            .as_deref()
            .filter(|value| !value.is_empty()),
        request
            .original_language
            .as_deref()
            .filter(|value| !value.is_empty()),
    );
    let preferred_subtitle_index = find_preferred_subtitle_index_in_tracks(
        &request.available_subtitles,
        request
            .last_subtitle_language
            .as_deref()
            .filter(|value| !value.is_empty()),
        request
            .preferred_subtitle_language
            .as_deref()
            .filter(|value| !value.is_empty()),
        request
            .secondary_subtitle_language
            .as_deref()
            .filter(|value| !value.is_empty()),
    );
    let preferred_subtitle_id = if preferred_subtitle_index >= 0 {
        request
            .available_subtitles
            .get(preferred_subtitle_index as usize)
            .and_then(|track| track.id.clone())
    } else {
        None
    };
    serde_json::to_string(&json!({
        "preferredAudioLanguage": preferred_audio_language,
        "preferredSubtitleIndex": preferred_subtitle_index,
        "preferredSubtitleId": preferred_subtitle_id,
        "subtitlesDisabled": preferred_subtitle_index < 0
    }))
    .ok()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamSelectionItem {
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
    addon_name: Option<String>,
    playable_url: Option<String>,
    binge_group: Option<String>,
    filename: Option<String>,
    effective_filename: Option<String>,
}

impl StreamSelectionItem {
    pub(crate) fn matches_episode(&self, video_id: &str) -> bool {
        stream_matches_episode(
            video_id,
            &[
                self.title.clone().unwrap_or_default(),
                self.name.clone().unwrap_or_default(),
                self.description.clone().unwrap_or_default(),
                self.filename.clone().unwrap_or_default(),
                self.effective_filename.clone().unwrap_or_default(),
            ],
        )
    }

    pub(crate) fn is_playable_for_episode(&self, video_id: &str) -> bool {
        self.playable_url
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && self.matches_episode(video_id)
    }

    pub(crate) fn selection_text(&self) -> String {
        [
            self.name.as_deref(),
            self.title.as_deref(),
            self.description.as_deref(),
            self.addon_name.as_deref(),
            self.playable_url.as_deref(),
            self.binge_group.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
    }
}

fn stream_selection_item_from_value(v: &Value) -> StreamSelectionItem {
    StreamSelectionItem {
        name: v.get("name").and_then(Value::as_str).map(str::to_string),
        title: v.get("title").and_then(Value::as_str).map(str::to_string),
        description: v
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        addon_name: v
            .get("addonName")
            .and_then(Value::as_str)
            .map(str::to_string),
        playable_url: v
            .get("playableUrl")
            .and_then(Value::as_str)
            .map(str::to_string),
        binge_group: v
            .get("bingeGroup")
            .and_then(Value::as_str)
            .map(str::to_string),
        filename: v
            .get("filename")
            .and_then(Value::as_str)
            .map(str::to_string),
        effective_filename: v
            .get("effectiveFilename")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

pub(crate) fn index_of_first_playable<F>(
    streams: &[StreamSelectionItem],
    video_id: &str,
    predicate: F,
) -> Option<usize>
where
    F: Fn(&StreamSelectionItem) -> bool,
{
    streams
        .iter()
        .position(|stream| stream.is_playable_for_episode(video_id) && predicate(stream))
}

pub(crate) fn manual_stream_index(
    streams: &[StreamSelectionItem],
    video_id: &str,
    initial_stream_index: i32,
    saved_url: Option<&str>,
    saved_title: Option<&str>,
) -> i32 {
    let matched_index = saved_url
        .filter(|value| !value.is_empty())
        .and_then(|value| {
            index_of_first_playable(streams, video_id, |stream| {
                stream.playable_url.as_deref() == Some(value)
            })
        })
        .or_else(|| {
            saved_title
                .filter(|value| !value.is_empty())
                .and_then(|value| {
                    index_of_first_playable(streams, video_id, |stream| {
                        stream.title.as_deref() == Some(value)
                    })
                })
        });
    if let Some(index) = matched_index {
        return index as i32;
    }

    if initial_stream_index >= 0
        && streams
            .get(initial_stream_index as usize)
            .is_some_and(|stream| stream.matches_episode(video_id))
    {
        return initial_stream_index;
    }

    streams
        .iter()
        .position(|stream| stream.matches_episode(video_id))
        .map(|index| index as i32)
        .unwrap_or(-1)
}

#[allow(clippy::too_many_arguments)]
fn select_stream_index_inner(
    streams: &[StreamSelectionItem],
    current_video_id: &str,
    initial_stream_index: i32,
    saved_url: Option<&str>,
    saved_title: Option<&str>,
    source_selection_mode: SourceSelectionMode,
    regex_pattern: Option<&str>,
    preferred_binge_group: Option<&str>,
) -> i32 {
    if streams.is_empty() {
        return -1;
    }

    if let Some(group) = preferred_binge_group.filter(|value| !value.trim().is_empty()) {
        if let Some(index) = index_of_first_playable(streams, current_video_id, |stream| {
            stream.binge_group.as_deref() == Some(group)
        }) {
            return index as i32;
        }
    }

    match source_selection_mode {
        SourceSelectionMode::Regex => {
            let Some(pattern) = regex_pattern.filter(|value| !value.trim().is_empty()) else {
                return manual_stream_index(
                    streams,
                    current_video_id,
                    initial_stream_index,
                    saved_url,
                    saved_title,
                );
            };
            let regex = match regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
            {
                Ok(regex) => regex,
                Err(_) => {
                    return manual_stream_index(
                        streams,
                        current_video_id,
                        initial_stream_index,
                        saved_url,
                        saved_title,
                    )
                }
            };
            if let Some(index) = index_of_first_playable(streams, current_video_id, |stream| {
                regex.is_match(&stream.selection_text())
            }) {
                return index as i32;
            }
        }
        SourceSelectionMode::First => {
            if let Some(index) = index_of_first_playable(streams, current_video_id, |_| true) {
                return index as i32;
            }
        }
        SourceSelectionMode::Manual => {}
    }

    manual_stream_index(
        streams,
        current_video_id,
        initial_stream_index,
        saved_url,
        saved_title,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn select_stream_index(
    streams_json: &str,
    current_video_id: &str,
    initial_stream_index: i32,
    saved_url: Option<&str>,
    saved_title: Option<&str>,
    source_selection_mode: SourceSelectionMode,
    regex_pattern: Option<&str>,
    preferred_binge_group: Option<&str>,
) -> i32 {
    let Ok(streams) = serde_json::from_str::<Vec<StreamSelectionItem>>(streams_json) else {
        return -1;
    };
    select_stream_index_inner(
        &streams,
        current_video_id,
        initial_stream_index,
        saved_url,
        saved_title,
        source_selection_mode,
        regex_pattern,
        preferred_binge_group,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn select_stream_index_values(
    streams: &[Value],
    current_video_id: &str,
    initial_stream_index: i32,
    saved_url: Option<&str>,
    saved_title: Option<&str>,
    source_selection_mode: SourceSelectionMode,
    regex_pattern: Option<&str>,
    preferred_binge_group: Option<&str>,
) -> i32 {
    let items: Vec<StreamSelectionItem> = streams
        .iter()
        .map(stream_selection_item_from_value)
        .collect();
    select_stream_index_inner(
        &items,
        current_video_id,
        initial_stream_index,
        saved_url,
        saved_title,
        source_selection_mode,
        regex_pattern,
        preferred_binge_group,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn track_state(request: Value) -> Value {
        serde_json::from_str(&player_track_state_json(&request.to_string()).unwrap()).unwrap()
    }

    #[test]
    fn player_track_state_uses_audio_memory_before_profile_preference() {
        let state = track_state(json!({
            "lastAudioLanguage": "tr",
            "preferredAudioLanguage": "en",
            "originalLanguage": "en"
        }));

        assert_eq!(state["preferredAudioLanguage"], "tr");
    }

    #[test]
    fn player_track_state_uses_japanese_original_for_english_anime_preference() {
        let state = track_state(json!({
            "preferredAudioLanguage": "en",
            "originalLanguage": "ja"
        }));

        assert_eq!(state["preferredAudioLanguage"], "ja");
    }

    #[test]
    fn player_track_state_selects_subtitle_memory_then_secondary() {
        let memory = track_state(json!({
            "availableSubtitles": [
                { "id": "en", "label": "English", "language": "en" },
                { "id": "tr", "label": "Turkish", "language": "tr" }
            ],
            "lastSubtitleLanguage": "tr",
            "preferredSubtitleLanguage": "en"
        }));
        assert_eq!(memory["preferredSubtitleIndex"], 1);
        assert_eq!(memory["preferredSubtitleId"], "tr");
        assert_eq!(memory["subtitlesDisabled"], false);

        let secondary = track_state(json!({
            "availableSubtitles": [
                { "id": "tr", "label": "Turkish", "language": "tr" }
            ],
            "preferredSubtitleLanguage": "en",
            "secondarySubtitleLanguage": "tr"
        }));
        assert_eq!(secondary["preferredSubtitleIndex"], 0);
        assert_eq!(secondary["preferredSubtitleId"], "tr");
    }

    #[test]
    fn player_track_state_disables_subtitles_when_no_preferred_match_exists() {
        let state = track_state(json!({
            "availableSubtitles": [
                { "id": "tr", "label": "Turkish", "language": "tr" }
            ],
            "preferredSubtitleLanguage": "en"
        }));

        assert_eq!(state["preferredSubtitleIndex"], -1);
        assert!(state["preferredSubtitleId"].is_null());
        assert_eq!(state["subtitlesDisabled"], true);
    }

    #[test]
    fn build_magnet_dedupes_addon_tracker_and_appends_fallbacks() {
        let magnet = build_magnet(
            "ABCDEF1234567890ABCDEF1234567890ABCDEF12",
            &["tracker:udp://tracker.example:1337/announce".to_string()],
        );

        assert!(magnet.starts_with("magnet:?xt=urn:btih:abcdef1234567890abcdef1234567890abcdef12"));
        assert_eq!(magnet.matches("tracker.example%3A1337").count(), 1);
        assert!(magnet.contains("opentrackr.org"));
    }

    #[test]
    fn stream_magnet_link_builds_from_info_hash_and_sources() {
        let stream = json!({
            "infoHash": "ABCDEF1234567890ABCDEF1234567890ABCDEF12",
            "sources": ["tracker:udp://tracker.example:1337/announce"],
        });
        let link = stream_magnet_link(&stream).unwrap();
        assert!(link.starts_with("magnet:?xt=urn:btih:abcdef1234567890abcdef1234567890abcdef12"));
    }

    #[test]
    fn stream_magnet_link_none_for_direct_http_stream() {
        let stream = json!({ "url": "https://cdn.example/video.mkv" });
        assert!(stream_magnet_link(&stream).is_none());
    }

    #[test]
    fn resolve_torrent_file_index_prefers_requested_then_filename_then_largest_video() {
        let stats = vec![
            TorrentFileStat {
                id: 1,
                path: "Show.S01E01.mkv".to_string(),
                length: 100,
            },
            TorrentFileStat {
                id: 2,
                path: "Show.S01E02.mkv".to_string(),
                length: 300,
            },
            TorrentFileStat {
                id: 3,
                path: "sample.txt".to_string(),
                length: 999_999,
            },
        ];

        // Addon-provided fileIdx wins outright, even though it doesn't match any stat.
        assert_eq!(
            resolve_torrent_file_index("title", Some(9), None, &stats),
            (Some(9), Some("requested".to_string()))
        );

        // No requested index, but a preferred filename matches by basename.
        assert_eq!(
            resolve_torrent_file_index("title", None, Some("Show.S01E01.mkv"), &stats),
            (Some(1), Some("filename".to_string()))
        );

        // No requested index or filename match — falls back to the largest *video* file,
        // ignoring the much larger non-video sample.txt.
        assert_eq!(
            resolve_torrent_file_index("title", None, None, &stats),
            (Some(2), Some("largest-video".to_string()))
        );

        assert_eq!(
            resolve_torrent_file_index("title", None, None, &[]),
            (None, None)
        );
    }

    #[test]
    fn percent_decode_component_decodes_escapes_and_survives_multibyte_input() {
        assert_eq!(percent_decode_component("Breaking%20Bad"), "Breaking Bad");
        // A literal '%' immediately before a multi-byte UTF-8 character used to
        // panic on a mid-character slice bound; it must now just pass through.
        assert_eq!(percent_decode_component("%xé"), "%xé");
    }
}
