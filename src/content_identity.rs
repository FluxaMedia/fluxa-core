use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

const TMDB_ID_PREFIX: &str = "tmdb:";

// pub rather than pub(crate): re-exported under fuzz_targets for the `fuzz/`
// crate (see lib.rs). Not part of the supported public API otherwise.
pub fn parse_episode_locator(raw: &str) -> Option<(String, i32, i32)> {
    let parts = raw
        .split(':')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() >= 3 {
        let season = parts[parts.len() - 2].parse::<i32>().ok()?;
        let episode = parts[parts.len() - 1].parse::<i32>().ok()?;
        let base_id = parts[..parts.len() - 2].join(":");
        if !base_id.is_empty() {
            return Some((base_id, season, episode));
        }
    }

    if let Some((season, episode, _)) = scan_compact_episode_codes(raw).into_iter().next() {
        return Some((String::new(), season, episode));
    }

    let parts = raw
        .split([':', '/', '-', '_'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() >= 3 {
        let season = parts[parts.len() - 2].parse::<i32>().ok()?;
        let episode = parts[parts.len() - 1].parse::<i32>().ok()?;
        return Some((parts[..parts.len() - 2].join(":"), season, episode));
    }
    None
}

// Scans `text` for every SxxExx-style code (case-insensitive), yielding each
// match's season, episode, and whether a digit immediately follows the parsed
// episode number — callers that match against a specific target use that to
// reject a longer digit run than what they're looking for (e.g. "S01E100"
// shouldn't count as a match for episode 10).
fn scan_compact_episode_codes(text: &str) -> Vec<(i32, i32, bool)> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut matches = Vec::new();
    for index in 0..bytes.len() {
        if bytes[index] != b's' {
            continue;
        }
        let mut cursor = index + 1;
        let season_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if season_start == cursor || cursor >= bytes.len() || bytes[cursor] != b'e' {
            continue;
        }
        let episode_start = cursor + 1;
        cursor = episode_start;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if episode_start == cursor {
            continue;
        }
        let season = lower[season_start..episode_start - 1].parse::<i32>().ok();
        let episode = lower[episode_start..cursor].parse::<i32>().ok();
        let next_is_digit = cursor < bytes.len() && bytes[cursor].is_ascii_digit();
        if let (Some(season), Some(episode)) = (season, episode) {
            matches.push((season, episode, next_is_digit));
        }
    }
    matches
}

pub(crate) fn imdb_id(raw: &str) -> Option<String> {
    imdb_regex()
        .find(raw)
        .map(|matched| matched.as_str().to_string())
}

pub(crate) fn base_content_id(id: &str) -> String {
    parse_episode_locator(id)
        .map(|(base_id, _, _)| {
            if base_id.is_empty() {
                id.to_string()
            } else {
                base_id
            }
        })
        .unwrap_or_else(|| id.to_string())
}

pub(crate) fn normalize_series_lookup_id(raw_id: &str) -> String {
    imdb_id(raw_id).unwrap_or_else(|| base_content_id(raw_id))
}

pub(crate) fn is_tmdb_like_content_id(id: &str) -> bool {
    let base = base_content_id(id);
    base.to_ascii_lowercase().starts_with(TMDB_ID_PREFIX) || base.parse::<i32>().is_ok()
}

pub(crate) fn episode_id(base_id: &str, season: i32, episode: i32) -> String {
    format!("{base_id}:{season}:{episode}")
}

pub(crate) fn stream_request_ids(
    content_type: &str,
    id: &str,
    detail_id: Option<&str>,
    current_series_lookup_id: Option<&str>,
    canonical_base_id: Option<&str>,
) -> Vec<String> {
    let mut ids = Vec::new();
    if content_type != "series" {
        if is_tmdb_like_content_id(id) {
            if let Some(canonical) = canonical_base_id {
                push_unique(&mut ids, canonical.to_string());
            }
        }
        push_unique(&mut ids, id.to_string());
        if let Some(detail) = detail_id {
            push_unique(&mut ids, detail.to_string());
        }
        if let Some(canonical) = canonical_base_id {
            push_unique(&mut ids, canonical.to_string());
        }
        return ids;
    }

    let locator = parse_episode_locator(id);
    let normalized_series_id = current_series_lookup_id
        .map(str::to_string)
        .or_else(|| detail_id.map(normalize_series_lookup_id));
    let normalized_detail_base_id = detail_id.map(base_content_id);

    if let Some((_, season, episode)) = locator {
        push_unique(&mut ids, id.to_string());
        if let Some(series_id) = normalized_series_id {
            push_unique(&mut ids, episode_id(&series_id, season, episode));
        }
        if let Some(detail_base_id) = normalized_detail_base_id {
            push_unique(&mut ids, episode_id(&detail_base_id, season, episode));
        }
        push_unique(&mut ids, episode_id(&base_content_id(id), season, episode));
        if let Some(canonical) = canonical_base_id {
            push_unique(&mut ids, episode_id(canonical, season, episode));
        }
    } else {
        push_unique(&mut ids, id.to_string());
        if let Some(series_id) = normalized_series_id {
            push_unique(&mut ids, series_id);
        }
        if let Some(detail) = detail_id {
            push_unique(&mut ids, detail.to_string());
        }
        if let Some(canonical) = canonical_base_id {
            push_unique(&mut ids, canonical.to_string());
        }
    }

    ids
}

pub(crate) fn playback_intro_lookup_content_id(id: &str) -> String {
    if let Some(imdb) = imdb_id(id) {
        return imdb;
    }
    base_content_id(id)
        .trim_start_matches(TMDB_ID_PREFIX)
        .to_string()
}

pub(crate) fn playback_stream_request_ids_json(
    content_type: &str,
    id: &str,
    detail_id: Option<&str>,
) -> Option<String> {
    let canonical_base_id = imdb_id(id).or_else(|| detail_id.and_then(imdb_id));
    serde_json::to_string(&stream_request_ids(
        content_type,
        id,
        detail_id,
        detail_id.map(normalize_series_lookup_id).as_deref(),
        canonical_base_id.as_deref(),
    ))
    .ok()
}

pub(crate) fn direct_playback_plan_json(
    meta_json: &str,
    detail_json: Option<&str>,
    today_iso: &str,
) -> Option<String> {
    let meta: Value = serde_json::from_str(meta_json).ok()?;
    let detail: Value = detail_json
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or(Value::Null);
    let has_detail = detail.as_object().is_some_and(|object| !object.is_empty());
    let playback_meta = if has_detail {
        home_playback_meta(&meta, &detail)
    } else {
        meta.clone()
    };
    let target_video_id = string_field(&meta, "lastVideoId")
        .or_else(|| select_direct_playback_video_id(&detail, today_iso));
    let lookup_id = target_video_id
        .clone()
        .or_else(|| string_field(&detail, "id"))
        .or_else(|| string_field(&meta, "id"))
        .unwrap_or_default();

    serde_json::to_string(&json!({
        "meta": playback_meta,
        "targetVideoId": target_video_id,
        "lookupId": lookup_id
    }))
    .ok()
}

fn home_playback_meta(fallback: &Value, detail: &Value) -> Value {
    let mut meta = Map::new();
    for key in [
        "id",
        "name",
        "type",
        "poster",
        "background",
        "logo",
        "description",
        "imdbRating",
        "ageRating",
        "ratings",
        "genres",
        "releaseInfo",
        "released",
        "runtime",
        "seasonsCount",
        "cast",
        "originalLanguage",
    ] {
        insert_detail_or_fallback(&mut meta, key, detail, fallback);
    }
    if meta
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.trim().is_empty())
    {
        meta.insert(
            "name".to_string(),
            fallback
                .get("name")
                .cloned()
                .unwrap_or(Value::String(String::new())),
        );
    }
    let episodes_count = detail
        .get("videos")
        .and_then(Value::as_array)
        .map(|videos| json!(videos.len()))
        .unwrap_or_else(|| {
            fallback
                .get("episodesCount")
                .cloned()
                .unwrap_or(Value::Null)
        });
    meta.insert("episodesCount".to_string(), episodes_count);
    for key in [
        "timeOffset",
        "duration",
        "lastVideoId",
        "lastStreamIndex",
        "lastEpisodeName",
        "lastStreamUrl",
        "lastStreamTitle",
        "lastAudioLanguage",
        "lastSubtitleLanguage",
        "awards",
        "rank",
        "reason",
        "homeBadge",
    ] {
        meta.insert(
            key.to_string(),
            fallback.get(key).cloned().unwrap_or(Value::Null),
        );
    }
    Value::Object(meta)
}

fn insert_detail_or_fallback(
    target: &mut Map<String, Value>,
    key: &str,
    detail: &Value,
    fallback: &Value,
) {
    target.insert(
        key.to_string(),
        detail
            .get(key)
            .filter(|value| !value.is_null())
            .cloned()
            .unwrap_or_else(|| fallback.get(key).cloned().unwrap_or(Value::Null)),
    );
}

fn select_direct_playback_video_id(detail: &Value, today_iso: &str) -> Option<String> {
    if detail.get("type").and_then(Value::as_str) != Some("series") {
        return None;
    }
    let mut videos = detail
        .get("videos")
        .and_then(Value::as_array)?
        .iter()
        .collect::<Vec<_>>();
    videos.sort_by_key(|video| {
        (
            number_field(video, "season").unwrap_or(i64::MAX),
            number_field(video, "number")
                .or_else(|| number_field(video, "episode"))
                .unwrap_or(i64::MAX),
        )
    });
    videos
        .iter()
        .find(|video| {
            !string_field(video, "released")
                .as_deref()
                .is_some_and(|released| is_upcoming_iso(released, today_iso))
        })
        .copied()
        .or_else(|| videos.first().copied())
        .and_then(|video| string_field(video, "id"))
}

fn is_upcoming_iso(value: &str, today_iso: &str) -> bool {
    let date = value.trim().get(0..10).unwrap_or("").to_string();
    date.len() == 10 && date.as_str() > today_iso
}

pub(crate) fn stream_discovery_episode_context_json(
    content_type: &str,
    request_id: &str,
    detail_json: Option<&str>,
    season_episodes_json: &str,
) -> Option<String> {
    let season_episodes: Vec<Value> =
        serde_json::from_str(season_episodes_json).unwrap_or_default();
    let detail: Value = detail_json
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or(Value::Null);

    let expected_episode_titles = if content_type == "series" {
        season_episodes
            .iter()
            .find(|episode| string_field(episode, "id").as_deref() == Some(request_id))
            .or_else(|| {
                detail
                    .get("videos")
                    .and_then(Value::as_array)
                    .and_then(|videos| {
                        videos.iter().find(|episode| {
                            string_field(episode, "id").as_deref() == Some(request_id)
                        })
                    })
            })
            .and_then(|episode| string_field(episode, "name"))
            .map(|title| vec![title])
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut season_episode_titles = Map::new();
    let mut season_episode_ids = Map::new();
    if content_type == "series" {
        for episode in &season_episodes {
            let Some(number) = number_field(episode, "number") else {
                continue;
            };
            if let Some(title) = string_field(episode, "name") {
                let key = number.to_string();
                let values = season_episode_titles
                    .entry(key)
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Some(values) = values.as_array_mut() {
                    if !values
                        .iter()
                        .any(|value| value.as_str() == Some(title.as_str()))
                    {
                        values.push(Value::String(title));
                    }
                }
            }
            if let Some(id) = string_field(episode, "id") {
                season_episode_ids
                    .entry(number.to_string())
                    .or_insert(Value::String(id));
            }
        }
    }

    serde_json::to_string(&serde_json::json!({
        "expectedEpisodeTitles": expected_episode_titles,
        "seasonEpisodeTitles": season_episode_titles,
        "seasonEpisodeIds": season_episode_ids
    }))
    .ok()
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn number_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

// pub rather than pub(crate): re-exported under fuzz_targets for the `fuzz/`
// crate (see lib.rs). Not part of the supported public API otherwise.
pub fn percent_decode_component(value: &str) -> String {
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

pub(crate) fn form_decode(value: &str) -> String {
    percent_decode_component(&value.replace('+', " "))
}

pub(crate) fn stable_feed_part(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut replaced = false;
    for ch in value.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            output.push(ch);
            replaced = false;
        } else if !replaced {
            output.push('_');
            replaced = true;
        }
    }
    output.trim_matches('_').to_string()
}

pub(crate) fn normalize_content_type(value: &str) -> Option<&'static str> {
    match value.to_lowercase().as_str() {
        "movie" | "movies" => Some("movie"),
        "series" | "tv" | "show" | "shows" => Some("series"),
        "anime" => Some("anime"),
        _ => None,
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn normalize_provider_search_text(value: &str) -> String {
    collapse_whitespace(
        &value
            .to_lowercase()
            .replace(['+', '-', '_'], " ")
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == ' ' {
                    ch
                } else {
                    ' '
                }
            })
            .collect::<String>(),
    )
}

pub(crate) fn provider_search_terms(provider: &str) -> Vec<String> {
    match provider.trim().to_lowercase().as_str() {
        "8" => vec!["netflix".to_string()],
        "9" => vec!["prime".to_string(), "amazon".to_string()],
        "337" => vec!["disney".to_string()],
        "49" => vec!["hbo".to_string(), "max".to_string()],
        "350" => vec!["apple".to_string()],
        _ => {
            let normalized = normalize_provider_search_text(provider);
            if normalized.is_empty() {
                Vec::new()
            } else {
                vec![normalized]
            }
        }
    }
}

pub(crate) fn parse_string_list(json: &str) -> Vec<String> {
    serde_json::from_str::<Option<Vec<String>>>(json)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn json_string_list(values: &[String]) -> Option<String> {
    serde_json::to_string(values).ok()
}

pub(crate) fn effective_metadata_feed_selection_json(
    selected_keys_json: &str,
    available_keys_json: &str,
) -> Option<String> {
    let selected = serde_json::from_str::<Option<Vec<String>>>(selected_keys_json)
        .ok()
        .flatten()?;
    let available = parse_string_list(available_keys_json);
    let filtered = selected
        .into_iter()
        .filter(|key| available.contains(key))
        .collect::<Vec<_>>();
    json_string_list(&filtered)
}

pub(crate) fn toggle_metadata_feed_json(
    selected_keys_json: &str,
    available_keys_json: &str,
    key: &str,
) -> Option<String> {
    let selected = serde_json::from_str::<Option<Vec<String>>>(selected_keys_json)
        .ok()
        .flatten();
    let current = selected.unwrap_or_else(|| parse_string_list(available_keys_json));
    let mut output = Vec::<String>::new();
    let mut contains = false;
    for item in current {
        if item == key {
            contains = true;
        } else if !output.contains(&item) {
            output.push(item);
        }
    }
    if !contains {
        output.push(key.to_string());
    }
    json_string_list(&output)
}

pub(crate) fn toggle_metadata_feed_limited_json(
    selected_keys_json: &str,
    available_keys_json: &str,
    key: &str,
    max_enabled: i32,
) -> Option<String> {
    let current = serde_json::from_str::<Option<Vec<String>>>(selected_keys_json)
        .ok()
        .flatten()
        .unwrap_or_else(|| parse_string_list(available_keys_json));
    let output: Vec<String> = if current.iter().any(|item| item == key) {
        current.into_iter().filter(|item| item != key).collect()
    } else {
        let mut appended = current;
        appended.push(key.to_string());
        let keep = max_enabled.max(0) as usize;
        appended
            .into_iter()
            .rev()
            .take(keep)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };
    json_string_list(&output)
}

pub(crate) fn set_metadata_feed_group_enabled_json(
    selected_keys_json: &str,
    available_keys_json: &str,
    group_keys_json: &str,
    enabled: bool,
) -> Option<String> {
    let current = serde_json::from_str::<Option<Vec<String>>>(selected_keys_json)
        .ok()
        .flatten()
        .unwrap_or_else(|| parse_string_list(available_keys_json));
    let group = parse_string_list(group_keys_json);
    let mut output = Vec::<String>::new();
    for item in current {
        if (enabled || !group.contains(&item)) && !output.contains(&item) {
            output.push(item);
        }
    }
    if enabled {
        for item in group {
            if !output.contains(&item) {
                output.push(item);
            }
        }
    }
    json_string_list(&output)
}

pub(crate) fn ordered_metadata_feed_keys(
    option_keys_json: &str,
    order_json: &str,
) -> Option<String> {
    let option_keys = parse_string_list(option_keys_json);
    let order = parse_string_list(order_json);
    let mut output = Vec::<String>::new();
    for key in order {
        if option_keys.contains(&key) && !output.contains(&key) {
            output.push(key);
        }
    }
    for key in option_keys {
        if !output.contains(&key) {
            output.push(key);
        }
    }
    json_string_list(&output)
}

pub(crate) fn move_metadata_feed_order_json(
    option_keys_json: &str,
    current_order_json: &str,
    key: &str,
    delta: i32,
) -> Option<String> {
    let ordered_json = ordered_metadata_feed_keys(option_keys_json, current_order_json)?;
    let mut keys = parse_string_list(&ordered_json);
    let Some(from) = keys.iter().position(|item| item == key) else {
        return json_string_list(&keys);
    };
    if keys.is_empty() {
        return json_string_list(&keys);
    }
    let to = (from as i32 + delta).clamp(0, keys.len() as i32 - 1) as usize;
    if from != to {
        let moved = keys.remove(from);
        keys.insert(to, moved);
    }
    json_string_list(&keys)
}

pub(crate) fn normalized_billboard_title(value: &str) -> String {
    collapse_whitespace(
        &value
            .to_lowercase()
            .replace('ç', "c")
            .replace('ğ', "g")
            .replace('ı', "i")
            .replace('ö', "o")
            .replace('ş', "s")
            .replace('ü', "u")
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == ' ' {
                    ch
                } else {
                    ' '
                }
            })
            .collect::<String>(),
    )
}

pub(crate) fn content_trakt_key_value(meta: &Value) -> String {
    trakt_identity_key(meta)
}

pub(crate) fn content_merge_keys_value(meta: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    let id = meta_text(meta, "id");
    push_unique(&mut keys, content_trakt_key_value(meta));
    push_unique(&mut keys, id.to_string());
    push_unique(&mut keys, base_content_id(id));
    if let Some(imdb) = imdb_id(id) {
        push_unique(&mut keys, imdb);
    }
    if let Some(key) = title_year_key(meta) {
        push_unique(&mut keys, key);
    }
    keys
}

pub(crate) fn content_watched_keys_value(meta: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    push_unique(&mut keys, content_trakt_key_value(meta));
    push_unique(&mut keys, meta_text(meta, "id").to_string());
    if let Some(key) = title_year_key(meta) {
        push_unique(&mut keys, key);
    }
    keys
}

pub(crate) fn content_trakt_key(meta_json: &str) -> Option<String> {
    let meta = serde_json::from_str::<Value>(meta_json).ok()?;
    Some(content_trakt_key_value(&meta))
}

pub(crate) fn content_trakt_keys_batch(metas_json: &str) -> Option<String> {
    let metas: Vec<Value> = serde_json::from_str(metas_json).ok()?;
    let keys: Vec<String> = metas.iter().map(content_trakt_key_value).collect();
    serde_json::to_string(&keys).ok()
}

pub(crate) fn content_watched_keys_batch(metas_json: &str) -> Option<String> {
    let metas: Vec<Value> = serde_json::from_str(metas_json).ok()?;
    let keys: Vec<Vec<String>> = metas.iter().map(content_watched_keys_value).collect();
    serde_json::to_string(&keys).ok()
}

pub(crate) fn content_keys_json(meta_json: &str, watched: bool) -> Option<String> {
    let meta = serde_json::from_str::<Value>(meta_json).ok()?;
    let keys = if watched {
        content_watched_keys_value(&meta)
    } else {
        content_merge_keys_value(&meta)
    };
    serde_json::to_string(&keys).ok()
}

pub(crate) fn episode_filename_candidate(stream_json: &str, video_id: &str) -> Option<String> {
    let (_, season, episode) = parse_episode_locator(video_id)?;
    let stream = serde_json::from_str::<Value>(stream_json).ok()?;
    for value in ["title", "description", "name"] {
        if let Some(text) = stream.get(value).and_then(Value::as_str) {
            for line in text.lines().map(str::trim) {
                if is_likely_video_file(line) && text_matches_episode(line, season, episode) {
                    return Some(line.to_string());
                }
            }
        }
    }
    None
}

pub(crate) fn is_likely_video_file(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    [".mkv", ".mp4", ".avi", ".webm", ".m4v", ".mov", ".ts"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamDiscoveryCacheKeyRequest {
    #[serde(rename = "type")]
    content_type: String,
    id: String,
    language: String,
    cs3_search_query: Option<String>,
    cs3_year: Option<i64>,
    cs3_original_name: Option<String>,
    addon_signatures: Vec<String>,
    cs3_plugin_names: Vec<String>,
}

pub(crate) fn java_string_hash(value: &str) -> i32 {
    let mut hash = 0i32;
    for unit in value.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(unit as i32);
    }
    hash
}

pub(crate) fn stream_discovery_cache_key(request_json: &str) -> Option<String> {
    let mut request = serde_json::from_str::<StreamDiscoveryCacheKeyRequest>(request_json).ok()?;
    let mut addon_signatures = request.addon_signatures;
    addon_signatures.sort();
    let mut cs3_plugin_names = request.cs3_plugin_names;
    cs3_plugin_names.sort();
    let search_query = request.cs3_search_query.unwrap_or_default();
    let original_name_hash = request
        .cs3_original_name
        .take()
        .filter(|value| value != &search_query)
        .map(|value| java_string_hash(&value).to_string())
        .unwrap_or_default();
    Some(
        [
            request.content_type,
            request.id,
            request.language,
            search_query,
            request
                .cs3_year
                .map(|value| value.to_string())
                .unwrap_or_default(),
            original_name_hash,
            addon_signatures.join("|"),
            cs3_plugin_names.join("|"),
        ]
        .join("|"),
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoverCatalogCacheKeyRequest {
    #[serde(rename = "type")]
    content_type: String,
    catalog_key: Option<String>,
    genre: Option<String>,
    year: Option<String>,
    rating: Option<f32>,
    provider: Option<String>,
    region: Option<String>,
    catalog_signatures: Vec<String>,
}

pub(crate) fn discover_catalog_cache_key(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<DiscoverCatalogCacheKeyRequest>(request_json).ok()?;
    Some(
        [
            request.content_type,
            request.catalog_key.unwrap_or_default(),
            request.genre.unwrap_or_default(),
            request.year.unwrap_or_default(),
            request
                .rating
                .map(|value| value.to_string())
                .unwrap_or_default(),
            request.provider.unwrap_or_default(),
            request.region.unwrap_or_default(),
            request.catalog_signatures.join(","),
        ]
        .join("|"),
    )
}

pub(crate) fn parse_extra_args_json(extra: &str) -> Option<String> {
    let mut map = Map::new();
    for part in extra.split('&') {
        let key = part.split_once('=').map(|(key, _)| key).unwrap_or(part);
        if key.is_empty() {
            continue;
        }
        let value = part.split_once('=').map(|(_, value)| value).unwrap_or("");
        map.insert(form_decode(key), Value::String(form_decode(value)));
    }
    serde_json::to_string(&Value::Object(map)).ok()
}

// pub rather than pub(crate): re-exported under fuzz_targets for the `fuzz/`
// crate (see lib.rs). Not part of the supported public API otherwise.
pub fn contains_compact_episode(text: &str, season: i32, episode: i32) -> bool {
    scan_compact_episode_codes(text)
        .into_iter()
        .any(|(s, e, next_is_digit)| s == season && e == episode && !next_is_digit)
}

// pub rather than pub(crate): re-exported under fuzz_targets for the `fuzz/`
// crate (see lib.rs). Not part of the supported public API otherwise.
pub fn contains_spaced_episode(text: &str, season: i32, episode: i32) -> bool {
    let lower = text.to_ascii_lowercase();
    let mut offset = 0;
    // `offset` can land exactly at lower.len() after a non-matching episode at the
    // very end of the string — .get() (rather than direct indexing) keeps that a
    // clean "nothing left to search" instead of a panic.
    while let Some(season_index) = lower.get(offset..).and_then(|rest| rest.find("season")) {
        let mut cursor = offset + season_index + "season".len();
        while lower
            .as_bytes()
            .get(cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            cursor += 1;
        }
        let season_start = cursor;
        while lower.as_bytes().get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if season_start == cursor || lower[season_start..cursor].parse::<i32>().ok() != Some(season)
        {
            offset = cursor.saturating_add(1);
            continue;
        }
        let Some(episode_word_index) = lower[cursor..].find("episode") else {
            return false;
        };
        cursor += episode_word_index + "episode".len();
        while lower
            .as_bytes()
            .get(cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            cursor += 1;
        }
        let episode_start = cursor;
        while lower.as_bytes().get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        let next_is_digit = lower.as_bytes().get(cursor).is_some_and(u8::is_ascii_digit);
        if episode_start != cursor
            && lower[episode_start..cursor].parse::<i32>().ok() == Some(episode)
            && !next_is_digit
        {
            return true;
        }
        offset = cursor.saturating_add(1);
    }
    false
}

pub(crate) fn text_matches_episode(text: &str, season: i32, episode: i32) -> bool {
    contains_compact_episode(text, season, episode)
        || contains_spaced_episode(text, season, episode)
}

fn dash_episode_matches(text: &str, episode: i32) -> Option<bool> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut found = false;
    for index in 1..bytes.len() {
        if bytes[index] != b'-' || !bytes[index - 1].is_ascii_whitespace() {
            continue;
        }
        let mut cursor = index + 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        let length = cursor.saturating_sub(start);
        if length == 0 || length > 3 {
            continue;
        }
        if cursor < bytes.len()
            && bytes[cursor].is_ascii_alphabetic()
            && !(bytes[cursor] == b'v'
                && cursor + 1 < bytes.len()
                && bytes[cursor + 1].is_ascii_digit())
        {
            continue;
        }
        let Ok(candidate) = lower[start..cursor].parse::<i32>() else {
            continue;
        };
        found = true;
        if candidate == episode {
            return Some(true);
        }
    }
    found.then_some(false)
}

pub(crate) fn stream_matches_episode(video_id: &str, fields: &[String]) -> bool {
    let Some((_, season, episode)) = parse_episode_locator(video_id) else {
        return true;
    };
    let text = fields
        .iter()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if text.trim().is_empty() {
        return true;
    }
    if text_matches_episode(&text, season, episode) {
        return true;
    }
    if let Some(matches) = dash_episode_matches(&text, episode) {
        return matches;
    }
    !contains_any_compact_episode(&text)
}

pub(crate) fn contains_any_compact_episode(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for index in 0..bytes.len() {
        if bytes[index] != b's' {
            continue;
        }
        let mut cursor = index + 1;
        let season_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if season_start == cursor || season_start + 2 < cursor {
            continue;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'e' {
            continue;
        }
        let episode_start = cursor + 1;
        cursor = episode_start;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if episode_start != cursor && cursor - episode_start <= 3 {
            return true;
        }
    }
    false
}

pub(crate) fn imdb_regex() -> &'static regex::Regex {
    pub(crate) static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| regex::Regex::new(r"tt\d+").expect("valid imdb regex"))
}

pub(crate) fn year_regex() -> &'static regex::Regex {
    pub(crate) static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| regex::Regex::new(r"\d{4}").expect("valid year regex"))
}

pub(crate) fn meta_text<'a>(meta: &'a Value, key: &str) -> &'a str {
    meta.get(key).and_then(Value::as_str).unwrap_or("")
}

pub(crate) fn normalized_loose_title(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub(crate) fn title_year_key(meta: &Value) -> Option<String> {
    let title = normalized_loose_title(meta_text(meta, "name"));
    if title.is_empty() {
        return None;
    }
    let year = year_regex()
        .find(meta_text(meta, "releaseInfo"))
        .map(|matched| matched.as_str())
        .unwrap_or("");
    Some(format!("{}:{title}:{year}", meta_text(meta, "type")))
}

pub(crate) fn trakt_identity_key(meta: &Value) -> String {
    let id = meta_text(meta, "id");
    if let Some(value) = imdb_regex().find(id).map(|matched| matched.as_str()) {
        return value.to_string();
    }
    let tmdb = if id.to_ascii_lowercase().starts_with(TMDB_ID_PREFIX) {
        id.strip_prefix(TMDB_ID_PREFIX).unwrap_or(id)
    } else {
        ""
    };
    if !tmdb.is_empty() {
        return format!("tmdb:{tmdb}");
    }
    format!(
        "{}:{}:{}",
        meta_text(meta, "type"),
        normalized_loose_title(meta_text(meta, "name")),
        meta_text(meta, "releaseInfo")
    )
}

pub(crate) fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.is_empty() && !values.contains(&value) {
        values.push(value);
    }
}

pub(crate) fn continue_watching_merge_keys(meta: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    push_unique(&mut keys, trakt_identity_key(meta));
    let id = meta_text(meta, "id");
    let base_id = if parse_episode_locator(id).is_some() {
        parse_episode_locator(id)
            .map(|(base_id, _, _)| {
                if base_id.is_empty() {
                    id.to_string()
                } else {
                    base_id
                }
            })
            .unwrap_or_else(|| id.to_string())
    } else {
        id.to_string()
    };
    push_unique(&mut keys, id.to_string());
    push_unique(&mut keys, base_id);
    if let Some(value) = imdb_regex().find(id).map(|matched| matched.as_str()) {
        push_unique(&mut keys, value.to_string());
    }
    if let Some(value) = title_year_key(meta) {
        push_unique(&mut keys, value);
    }
    keys
}

pub(crate) fn is_trakt_continue_watching_source(meta: &Value) -> bool {
    meta_text(meta, "reason").eq_ignore_ascii_case("Trakt.tv")
}

pub(crate) fn merge_continue_watching_duplicates_json(items_json: &str) -> Option<String> {
    let items = serde_json::from_str::<Vec<Value>>(items_json).ok()?;
    let mut merged: Vec<Value> = Vec::new();
    let mut key_to_index: HashMap<String, usize> = HashMap::new();
    let mut aliases: HashMap<String, String> = HashMap::new();

    for item in items {
        let item_keys = continue_watching_merge_keys(&item);
        let key = item_keys
            .iter()
            .find_map(|item_key| aliases.get(item_key).cloned())
            .or_else(|| item_keys.first().cloned())
            .unwrap_or_default();
        if key.is_empty() {
            merged.push(item);
            continue;
        }

        if let Some(index) = key_to_index.get(&key).copied() {
            if is_trakt_continue_watching_source(&item)
                || !is_trakt_continue_watching_source(&merged[index])
            {
                merged[index] = item;
            }
        } else {
            key_to_index.insert(key.clone(), merged.len());
            merged.push(item);
        }
        for item_key in item_keys {
            aliases.insert(item_key, key.clone());
        }
    }

    serde_json::to_string(&merged).ok()
}

pub(crate) fn meta_year(meta: &Value) -> Option<String> {
    ["released", "releaseInfo"].into_iter().find_map(|key| {
        year_regex()
            .find(meta_text(meta, key))
            .map(|matched| matched.as_str().to_string())
    })
}

pub(crate) fn rating_value(value: &Value) -> Option<f32> {
    let text = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => value.to_string().trim_matches('"').trim().to_string(),
    };
    if text.is_empty() {
        return None;
    }
    if let Some((score, scale)) = text.split_once('/') {
        let score = score.trim().parse::<f32>().ok()?;
        let scale = scale.trim().parse::<f32>().ok()?;
        if scale == 0.0 {
            None
        } else {
            Some((score / scale) * 10.0)
        }
    } else if let Some(percent) = text.strip_suffix('%') {
        percent.trim().parse::<f32>().ok().map(|value| value / 10.0)
    } else {
        text.parse::<f32>().ok()
    }
}

pub(crate) fn meta_rating(meta: &Value) -> Option<f32> {
    meta.get("imdbRating")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f32>().ok())
        .or_else(|| {
            meta.get("ratings")
                .and_then(Value::as_array)
                .and_then(|ratings| {
                    ratings
                        .iter()
                        .find_map(|rating| rating.get("value").and_then(rating_value))
                })
        })
}

pub(crate) fn matches_discover_year(meta: &Value, year: Option<&str>) -> bool {
    let Some(expected) = year.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    meta_year(meta).as_deref() == Some(expected)
}

pub(crate) fn matches_discover_rating(meta: &Value, minimum_rating: Option<f32>) -> bool {
    let Some(minimum) = minimum_rating else {
        return true;
    };
    meta_rating(meta).is_some_and(|candidate| candidate >= minimum)
}

pub(crate) fn matches_discover_region(meta: &Value, region: Option<&str>) -> bool {
    let Some(expected) = region
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
    else {
        return true;
    };
    let language = meta_text(meta, "originalLanguage").to_lowercase();
    if language.is_empty() {
        return false;
    }
    match expected.as_str() {
        "us" | "usa" | "en" => language == "en",
        "jp" | "ja" | "japan" => language == "ja",
        "kr" | "ko" | "korea" => language == "ko",
        _ => language == expected,
    }
}

pub(crate) fn filter_discover_results_json(
    items_json: &str,
    year: Option<&str>,
    rating: Option<f32>,
    region: Option<&str>,
) -> Option<String> {
    let items = serde_json::from_str::<Vec<Value>>(items_json).ok()?;
    let filtered = items
        .into_iter()
        .filter(|item| matches_discover_year(item, year))
        .filter(|item| matches_discover_rating(item, rating))
        .filter(|item| matches_discover_region(item, region))
        .collect::<Vec<_>>();
    serde_json::to_string(&filtered).ok()
}

pub(crate) fn parse_video_id_json(id: &str) -> String {
    let parts: Vec<&str> = id.split(':').collect();
    let mut map = serde_json::Map::new();
    if parts.first().map(|p| p.starts_with("tt")).unwrap_or(false) {
        map.insert("imdb".into(), parts[0].into());
        if parts.len() >= 3 {
            if let (Ok(s), Ok(e)) = (parts[1].parse::<i64>(), parts[2].parse::<i64>()) {
                map.insert("season".into(), s.into());
                map.insert("episode".into(), e.into());
                map.insert("isEpisode".into(), true.into());
            } else {
                map.insert("isEpisode".into(), false.into());
            }
        } else {
            map.insert("isEpisode".into(), false.into());
        }
    } else if parts.first().map(|p| *p == "tmdb").unwrap_or(false) && parts.len() >= 2 {
        map.insert("tmdb".into(), parts[1].into());
        if parts.len() >= 4 {
            if let (Ok(s), Ok(e)) = (parts[2].parse::<i64>(), parts[3].parse::<i64>()) {
                map.insert("season".into(), s.into());
                map.insert("episode".into(), e.into());
                map.insert("isEpisode".into(), true.into());
            } else {
                map.insert("isEpisode".into(), false.into());
            }
        } else {
            map.insert("isEpisode".into(), false.into());
        }
    } else {
        map.insert("isEpisode".into(), false.into());
    }
    serde_json::to_string(&serde_json::Value::Object(map))
        .unwrap_or_else(|_| r#"{"isEpisode":false}"#.to_string())
}

pub(crate) fn build_trakt_ids_json(video_id: &str) -> Option<String> {
    let parsed_json = parse_video_id_json(video_id);
    let parsed: serde_json::Value = serde_json::from_str(&parsed_json).ok()?;
    if let Some(imdb) = parsed.get("imdb").and_then(serde_json::Value::as_str) {
        return serde_json::to_string(&serde_json::json!({"imdb": imdb})).ok();
    }
    if let Some(tmdb) = parsed.get("tmdb").and_then(serde_json::Value::as_str) {
        if let Ok(n) = tmdb.parse::<i64>() {
            return serde_json::to_string(&serde_json::json!({"tmdb": n})).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_intro_lookup_prefers_imdb_then_base_tmdb_number() {
        assert_eq!(playback_intro_lookup_content_id("tmdb:42:1:2"), "42");
        assert_eq!(
            playback_intro_lookup_content_id("tt1234567:1:2"),
            "tt1234567"
        );
    }

    #[test]
    fn stream_matching_uses_torrent_filename_episode_suffix() {
        let e1 = "[SubsPlease] Sousou no Frieren - 01v2 (1080p) [AAA94036].mkv".to_string();
        let e2 = "[SubsPlease] Sousou no Frieren - 02v2 (1080p) [00DB7386].mkv".to_string();
        assert!(!stream_matches_episode("tt123:1:2", &[e1]));
        assert!(stream_matches_episode("tt123:1:2", &[e2]));
    }

    #[test]
    fn playback_stream_request_ids_use_detail_imdb_as_canonical_id() {
        let ids = playback_stream_request_ids_json("movie", "tmdb:42", Some("tt1234567"))
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
            .expect("ids");

        assert_eq!(ids, vec!["tt1234567", "tmdb:42"]);
    }

    #[test]
    fn direct_playback_plan_selects_first_released_episode_without_mutating_provider_streams() {
        let plan = direct_playback_plan_json(
            r#"{"id":"tt1","name":"Fallback","type":"series","description":"fallback","lastStreamIndex":3}"#,
            Some(
                r#"{"id":"tt1","name":"Detail","type":"series","poster":"p","videos":[{"id":"tt1:1:2","season":1,"number":2,"released":"2026-06-01T00:00:00.000Z"},{"id":"tt1:1:1","season":1,"number":1,"released":"2026-05-01T00:00:00.000Z"}]}"#,
            ),
            "2026-05-21",
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("plan");

        assert_eq!(plan["targetVideoId"], "tt1:1:1");
        assert_eq!(plan["lookupId"], "tt1:1:1");
        assert_eq!(plan["meta"]["name"], "Detail");
        assert_eq!(plan["meta"]["description"], "fallback");
        assert_eq!(plan["meta"]["episodesCount"], 2);
        assert_eq!(plan["meta"]["lastStreamIndex"], 3);
    }

    #[test]
    fn direct_playback_plan_prefers_saved_video_and_falls_back_to_meta_without_detail() {
        let plan = direct_playback_plan_json(
            r#"{"id":"tt1","name":"Movie","type":"movie","lastVideoId":"tt1:2:3"}"#,
            None,
            "2026-05-21",
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("plan");

        assert_eq!(plan["targetVideoId"], "tt1:2:3");
        assert_eq!(plan["lookupId"], "tt1:2:3");
        assert_eq!(plan["meta"]["name"], "Movie");
    }

    #[test]
    fn effective_metadata_feed_selection_preserves_explicit_empty_selection() {
        assert_eq!(
            effective_metadata_feed_selection_json("null", r#"["a","b"]"#),
            None
        );
        assert_eq!(
            effective_metadata_feed_selection_json("[]", r#"["a","b"]"#).as_deref(),
            Some("[]")
        );
        assert_eq!(
            effective_metadata_feed_selection_json(r#"["old"]"#, r#"["a","b"]"#).as_deref(),
            Some("[]")
        );
        assert_eq!(
            effective_metadata_feed_selection_json(r#"["a","old"]"#, r#"["a","b"]"#).as_deref(),
            Some(r#"["a"]"#)
        );
    }

    #[test]
    fn stream_discovery_episode_context_preserves_episode_order() {
        let context = stream_discovery_episode_context_json(
            "series",
            "tt1:1:2",
            Some(r#"{"videos":[{"id":"tt1:1:2","name":"From detail"}]}"#),
            r#"[{"id":"tt1:1:1","number":1,"name":"Pilot"},{"id":"tt1:1:2","number":2,"name":"Second"}]"#,
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("context");

        assert_eq!(
            context
                .get("expectedEpisodeTitles")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(Value::as_str),
            Some("Second")
        );
        assert_eq!(
            context
                .get("seasonEpisodeIds")
                .and_then(Value::as_object)
                .and_then(|ids| ids.get("2"))
                .and_then(Value::as_str),
            Some("tt1:1:2")
        );
    }

    #[test]
    fn parse_episode_locator_finds_compact_code_with_no_colon_separators() {
        assert_eq!(
            parse_episode_locator("Show.Name.S01E02.1080p"),
            Some((String::new(), 1, 2))
        );
    }

    #[test]
    fn contains_compact_episode_rejects_longer_digit_run_than_target() {
        // "S01E100" must not match a search for episode 10 — the digit run
        // continues past what was parsed for the target.
        assert!(!contains_compact_episode("Show.S01E100.mkv", 1, 10));
        assert!(contains_compact_episode("Show.S01E100.mkv", 1, 100));
        assert!(contains_compact_episode("Show.S01E02.mkv", 1, 2));
    }

    #[test]
    fn contains_spaced_episode_matches_word_form_and_skips_wrong_season_occurrence() {
        assert!(contains_spaced_episode(
            "Show Name Season 1 Episode 2 1080p",
            1,
            2
        ));
        // First "Season 2" occurrence doesn't match the target season (1), so the
        // scan must continue past it to the second "Season ... Episode ..." pair.
        assert!(contains_spaced_episode(
            "Season 2 Episode 1 and Season 1 Episode 2",
            1,
            2
        ));
        // "Episode 10" must not match a search for episode 1 — same digit-run guard
        // as the compact S01E01 form.
        assert!(!contains_spaced_episode("Season 1 Episode 10", 1, 1));
        assert!(contains_spaced_episode("Season 1 Episode 10", 1, 10));
        // A season number that matches but with no "Episode" anywhere after it.
        assert!(!contains_spaced_episode(
            "Season 1 has no further structure",
            1,
            2
        ));
    }

    #[test]
    fn percent_decode_component_decodes_escapes_and_survives_multibyte_input() {
        assert_eq!(percent_decode_component("a%2Bb"), "a+b");
        // Same fix as the duplicate copy in stream_policy.rs — a '%' next to a
        // multi-byte UTF-8 character used to panic on a mid-character slice.
        assert_eq!(percent_decode_component("%xé"), "%xé");
    }
}
