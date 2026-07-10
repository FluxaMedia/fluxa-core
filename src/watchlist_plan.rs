use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TogglePlanRequest {
    item: Value,
    #[serde(default)]
    is_currently_in_watchlist: bool,
    #[serde(default)]
    profile_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalMergeRequest {
    #[serde(default)]
    local_items: Vec<Value>,
    #[serde(default)]
    external_items: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollectionImportRequest {
    #[serde(default)]
    collections: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfflineGroupingRequest {
    #[serde(default)]
    items: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgressMergeRequest {
    existing: Value,
    incoming: Value,
}

pub(crate) fn watchlist_toggle_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<TogglePlanRequest>(request_json).ok()?;
    let is_in_watchlist = request.is_currently_in_watchlist;
    let should_add = !is_in_watchlist;
    let item_id = request
        .item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    serde_json::to_string(&json!({
        "command": if should_add { "add" } else { "remove" },
        "itemId": item_id,
        "optimisticIsInWatchlist": should_add,
        "profileId": request.profile_id
    }))
    .ok()
}

pub(crate) fn library_external_merge_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ExternalMergeRequest>(request_json).ok()?;
    let local_ids: std::collections::HashSet<String> = request
        .local_items
        .iter()
        .filter_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();
    let merged_external: Vec<&Value> = request
        .external_items
        .iter()
        .filter(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !local_ids.contains(id))
        })
        .collect();
    let mut merged: Vec<Value> = request.local_items.clone();
    merged.extend(merged_external.into_iter().cloned());
    serde_json::to_string(&json!({
        "merged": merged,
        "localCount": request.local_items.len(),
        "externalOnlyCount": merged.len() - request.local_items.len()
    }))
    .ok()
}

pub(crate) fn library_collection_import_validation_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<CollectionImportRequest>(request_json).ok()?;
    let mut issues = Vec::<String>::new();
    let mut valid_collections = Vec::<Value>::new();
    for (i, col) in request.collections.iter().enumerate() {
        let id = col.get("id").and_then(Value::as_str).unwrap_or("").trim();
        let title = col
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            issues.push(format!("collection[{}]: missing id", i));
            continue;
        }
        if title.is_empty() {
            issues.push(format!("collection[{}]: missing title", i));
            continue;
        }
        valid_collections.push(col.clone());
    }
    serde_json::to_string(&json!({
        "isValid": issues.is_empty(),
        "validCollections": valid_collections,
        "issues": issues
    }))
    .ok()
}

pub(crate) fn library_offline_grouping_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<OfflineGroupingRequest>(request_json).ok()?;
    let mut ready = Vec::<&Value>::new();
    let mut downloading = Vec::<&Value>::new();
    let mut queued = Vec::<&Value>::new();
    let mut failed = Vec::<&Value>::new();
    for item in &request.items {
        match item.get("status").and_then(Value::as_str).unwrap_or("") {
            "ready" | "complete" => ready.push(item),
            "downloading" | "in_progress" => downloading.push(item),
            "failed" | "error" => failed.push(item),
            _ => queued.push(item),
        }
    }
    serde_json::to_string(&json!({
        "ready": ready,
        "downloading": downloading,
        "queued": queued,
        "failed": failed
    }))
    .ok()
}

pub(crate) fn playback_progress_merge_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ProgressMergeRequest>(request_json).ok()?;
    let existing = &request.existing;
    let incoming = &request.incoming;

    let existing_video_id = existing.get("lastVideoId").and_then(Value::as_str);
    let incoming_video_id = incoming.get("lastVideoId").and_then(Value::as_str);
    let video_changed = incoming_video_id.is_some() && incoming_video_id != existing_video_id;

    let resolve_field = |key: &str| -> Value {
        incoming
            .get(key)
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| existing.get(key).cloned())
            .unwrap_or(Value::Null)
    };

    serde_json::to_string(&json!({
        "lastVideoId": resolve_field("lastVideoId"),
        "timeOffset": incoming.get("timeOffset").cloned().unwrap_or(Value::Null),
        "duration": incoming.get("duration").cloned().unwrap_or(Value::Null),
        "lastStreamIndex": resolve_field("lastStreamIndex"),
        "lastEpisodeName": resolve_field("lastEpisodeName"),
        "lastEpisodeSeason": resolve_field("lastEpisodeSeason"),
        "lastEpisodeNumber": resolve_field("lastEpisodeNumber"),
        "lastEpisodeThumbnail": resolve_field("lastEpisodeThumbnail"),
        "lastStreamUrl": resolve_field("lastStreamUrl"),
        "lastStreamTitle": resolve_field("lastStreamTitle"),
        "continueWatchingPoster": resolve_field("continueWatchingPoster"),
        "continueWatchingBackground": resolve_field("continueWatchingBackground"),
        "lastAudioLanguage": resolve_field("lastAudioLanguage"),
        "lastSubtitleLanguage": resolve_field("lastSubtitleLanguage"),
        "videoChanged": video_changed
    }))
    .ok()
}

fn cleaned_url(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn github_blob_url_regex() -> &'static regex::Regex {
    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        regex::Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/blob/([^/]+)/(.+)$")
            .expect("valid github blob url regex")
    })
}

fn cleaned_artwork_url(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim().trim_matches('\'').trim_matches('"').trim();
    if s.is_empty() {
        return None;
    }
    let with_scheme = if s.starts_with("//") {
        format!("https:{s}")
    } else {
        s.to_string()
    };
    let normalized = if let Some(caps) = github_blob_url_regex().captures(&with_scheme) {
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            &caps[1], &caps[2], &caps[3], &caps[4]
        )
    } else {
        with_scheme
    };
    Some(normalized.replace(' ', "%20"))
}

fn pick_str<'a>(obj: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(Value::String(s)) = obj.get(*k) {
            return Some(s.as_str());
        }
    }
    None
}

fn normalize_shape(value: Option<&str>) -> &'static str {
    match value.map(|s| s.trim().to_uppercase()).as_deref() {
        Some("LANDSCAPE") | Some("WIDE") => "wide",
        Some("SQUARE") => "square",
        _ => "poster",
    }
}

fn export_shape(value: Option<&str>) -> &'static str {
    match value.map(str::to_lowercase).as_deref() {
        Some("wide") | Some("landscape") => "LANDSCAPE",
        Some("square") => "SQUARE",
        _ => "POSTER",
    }
}

// FNV-1a over the title: a wasm-safe, deterministic id suffix for imported
// entries that arrive without one (re-importing the same file is idempotent).
fn stable_suffix(seed: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub(crate) fn import_collections_json(raw_json: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(raw_json).ok()?;
    let arr: Vec<&Value> = match parsed.as_array() {
        Some(a) => a.iter().collect(),
        None => vec![&parsed],
    };

    let collections: Vec<Value> = arr.iter().enumerate().filter_map(|(i, col)| {
        let col = col.as_object()?;
        let title = col.get("title")?.as_str()?.trim().to_string();
        if title.is_empty() { return None; }
        let id = col.get("id").and_then(Value::as_str).filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("imported_{}_{i}", stable_suffix(&title)));

        let raw_folders = col.get("folders").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
        let folders: Vec<Value> = raw_folders.iter().enumerate().filter_map(|(fi, f)| {
            let folder = f.as_object()?;
            let folder_title = folder.get("title")?.as_str()?.trim().to_string();
            if folder_title.is_empty() { return None; }
            let fid = folder.get("id").and_then(Value::as_str).filter(|s| !s.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("folder_{}_{fi}", stable_suffix(&folder_title)));

            let raw_sources = folder.get("catalogSources").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
            let mut sources: Vec<Value> = raw_sources.iter().filter_map(|s| {
                let o = s.as_object()?;
                let catalog_id = o.get("catalogId")?.as_str().filter(|s| !s.is_empty())?;
                Some(json!({
                    "catalogId": catalog_id,
                    "type": o.get("type").and_then(Value::as_str).unwrap_or("movie"),
                    "addonId": o.get("addonId").and_then(Value::as_str),
                }))
            }).collect();

            if sources.is_empty() {
                if let Some(fallback_id) = folder.get("catalogId").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                    sources.push(json!({ "catalogId": fallback_id, "type": "movie" }));
                }
            }

            let cover_image_url = cleaned_artwork_url(pick_str(folder, &["coverImageUrl","coverUrl","coverImage","cover","poster","thumbnail","thumb"]));
            let image_url = cleaned_artwork_url(pick_str(folder, &["imageUrl","image","image_url","posterUrl","poster_url"]));
            let effective_cover = cover_image_url.or(image_url);
            let hero_backdrop_url = cleaned_url(pick_str(folder, &["heroBackdropUrl","background","backdrop","backgroundUrl","backdropUrl"]));
            let shape = normalize_shape(folder.get("tileShape").or(folder.get("shape")).and_then(Value::as_str));

            Some(json!({
                "id": fid,
                "title": folder_title,
                "catalogTitle": folder.get("catalogTitle").and_then(Value::as_str).unwrap_or(&folder_title),
                "catalogId": sources.first().and_then(|s| s.get("catalogId")).and_then(Value::as_str),
                "genre": folder.get("genre").and_then(Value::as_str),
                "shape": shape,
                "hideTitle": folder.get("hideTitle").and_then(Value::as_bool).unwrap_or(false),
                "focusGifEnabled": folder.get("focusGifEnabled").and_then(Value::as_bool).unwrap_or(true),
                "catalogSources": if sources.is_empty() { Value::Null } else { json!(sources) },
                "coverEmoji": folder.get("coverEmoji").and_then(Value::as_str),
                "imageUrl": effective_cover,
                "coverImageUrl": effective_cover,
                "focusGifUrl": cleaned_url(folder.get("focusGifUrl").and_then(Value::as_str)),
                "titleLogoUrl": cleaned_url(folder.get("titleLogoUrl").and_then(Value::as_str)),
                "heroBackdropUrl": hero_backdrop_url,
            }))
        }).collect();

        let first_folder_cover = raw_folders.first()
            .and_then(|f| f.as_object())
            .and_then(|f| cleaned_artwork_url(pick_str(f, &["coverImageUrl","coverUrl","coverImage","cover","poster","thumbnail","thumb"]))
                .or_else(|| cleaned_artwork_url(pick_str(f, &["imageUrl","image","image_url","posterUrl","poster_url"]))));

        Some(json!({
            "id": id,
            "title": title,
            "imageUrl": first_folder_cover,
            "showOnHome": col.get("showOnHome").and_then(Value::as_bool).unwrap_or(true),
            "itemIds": [],
            "folders": folders,
            "showAllTab": col.get("showAllTab").and_then(Value::as_bool).unwrap_or(true),
            "viewMode": col.get("viewMode").and_then(Value::as_str).unwrap_or("FOLLOW_LAYOUT"),
            "pinToTop": col.get("pinToTop").and_then(Value::as_bool).unwrap_or(false),
            "focusGlowEnabled": col.get("focusGlowEnabled").and_then(Value::as_bool).unwrap_or(true),
        }))
    }).collect();

    serde_json::to_string(&collections).ok()
}

pub(crate) fn export_collections_json(collections_json: &str) -> Option<String> {
    let collections: Vec<Value> = serde_json::from_str(collections_json).ok()?;
    let data: Vec<Value> = collections.iter().map(|col| {
        let folders_raw = col.get("folders").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
        let folders: Vec<Value> = folders_raw.iter().map(|folder| {
            let catalog_sources: Vec<Value> = folder.get("catalogSources")
                .and_then(Value::as_array)
                .filter(|arr| !arr.is_empty()).cloned()
                .unwrap_or_else(|| {
                    if let Some(cid) = folder.get("catalogId").and_then(Value::as_str) {
                        vec![json!({ "catalogId": cid, "type": "movie" })]
                    } else {
                        vec![]
                    }
                });
            json!({
                "id": folder.get("id"),
                "title": folder.get("title"),
                "tileShape": export_shape(folder.get("shape").and_then(Value::as_str)),
                "hideTitle": folder.get("hideTitle").and_then(Value::as_bool).unwrap_or(false),
                "focusGifEnabled": folder.get("focusGifEnabled").and_then(Value::as_bool).unwrap_or(true),
                "catalogSources": catalog_sources,
                "coverEmoji": folder.get("coverEmoji"),
                "coverImageUrl": folder.get("coverImageUrl").or_else(|| folder.get("imageUrl")),
                "focusGifUrl": folder.get("focusGifUrl"),
                "titleLogoUrl": folder.get("titleLogoUrl"),
                "heroBackdropUrl": folder.get("heroBackdropUrl"),
            })
        }).collect();
        json!({
            "id": col.get("id"),
            "title": col.get("title"),
            "showAllTab": col.get("showAllTab").and_then(Value::as_bool).unwrap_or(true),
            "viewMode": col.get("viewMode").and_then(Value::as_str).unwrap_or("FOLLOW_LAYOUT"),
            "showOnHome": col.get("showOnHome").and_then(Value::as_bool).unwrap_or(true),
            "pinToTop": col.get("pinToTop").and_then(Value::as_bool).unwrap_or(false),
            "focusGlowEnabled": col.get("focusGlowEnabled").and_then(Value::as_bool).unwrap_or(true),
            "folders": folders,
        })
    }).collect();
    serde_json::to_string(&data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn toggle_plan_adds_when_not_in_watchlist() {
        let result: Value = serde_json::from_str(
            &watchlist_toggle_plan_json(
                r#"{"item":{"id":"tt1","type":"movie"},"isCurrentlyInWatchlist":false}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["command"], "add");
        assert_eq!(result["optimisticIsInWatchlist"], true);
    }

    #[test]
    fn toggle_plan_removes_when_in_watchlist() {
        let result: Value = serde_json::from_str(
            &watchlist_toggle_plan_json(
                r#"{"item":{"id":"tt1","type":"movie"},"isCurrentlyInWatchlist":true}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["command"], "remove");
        assert_eq!(result["optimisticIsInWatchlist"], false);
    }

    #[test]
    fn external_merge_deduplicates_preferring_local() {
        let result: Value = serde_json::from_str(
            &library_external_merge_plan_json(
                r#"{"localItems":[{"id":"tt1","source":"local"}],"externalItems":[{"id":"tt1","source":"external"},{"id":"tt2","source":"external"}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let merged = result["merged"].as_array().unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0]["source"], "local");
        assert_eq!(merged[1]["source"], "external");
        assert_eq!(merged[1]["id"], "tt2");
    }

    #[test]
    fn collection_import_validation_rejects_missing_id() {
        let result: Value = serde_json::from_str(
            &library_collection_import_validation_json(
                r#"{"collections":[{"title":"My List"},{"id":"c1","title":"Valid"}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["isValid"], false);
        assert_eq!(result["validCollections"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn offline_grouping_partitions_by_status() {
        let result: Value = serde_json::from_str(
            &library_offline_grouping_json(
                r#"{"items":[{"id":"a","status":"ready"},{"id":"b","status":"downloading"},{"id":"c","status":"failed"},{"id":"d"}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["ready"].as_array().unwrap().len(), 1);
        assert_eq!(result["downloading"].as_array().unwrap().len(), 1);
        assert_eq!(result["failed"].as_array().unwrap().len(), 1);
        assert_eq!(result["queued"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn progress_merge_preserves_existing_fields_when_incoming_is_null() {
        let result: Value = serde_json::from_str(
            &playback_progress_merge_plan_json(
                r#"{"existing":{"lastStreamUrl":"http://old","lastVideoId":"v1","timeOffset":1000},"incoming":{"lastVideoId":"v1","timeOffset":2000,"duration":5000}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["timeOffset"], 2000);
        assert_eq!(result["lastStreamUrl"], "http://old");
        assert_eq!(result["videoChanged"], false);
    }

    #[test]
    fn progress_merge_keeps_prior_episode_number_on_video_change_with_incomplete_incoming() {
        let result: Value = serde_json::from_str(
            &playback_progress_merge_plan_json(
                r#"{"existing":{"lastVideoId":"v1","lastEpisodeSeason":1,"lastEpisodeNumber":5,"lastEpisodeName":"Old Name"},"incoming":{"lastVideoId":"v2","timeOffset":0,"duration":5000}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["videoChanged"], true);
        assert_eq!(result["lastEpisodeSeason"], 1);
        assert_eq!(result["lastEpisodeNumber"], 5);
        assert_eq!(result["lastEpisodeName"], "Old Name");
    }
}

pub(crate) fn library_apply_mark_watched_json(
    lib_json: &str,
    video_ids_json: &str,
) -> Option<String> {
    use crate::library_state::{
        build_continue_watching_from_progress_json, remember_last_watched_episodes_json,
    };

    let updated_lib_str = remember_last_watched_episodes_json(lib_json, video_ids_json);
    let mut lib: serde_json::Map<String, Value> = serde_json::from_str(&updated_lib_str).ok()?;

    let video_ids: Vec<String> = serde_json::from_str(video_ids_json).unwrap_or_default();
    let watched: std::collections::HashSet<&str> = video_ids.iter().map(String::as_str).collect();

    if let Some(ext_cw) = lib
        .get("externalContinueWatching")
        .and_then(Value::as_array)
        .cloned()
    {
        let filtered: Vec<Value> = ext_cw
            .into_iter()
            .filter(|item| {
                let last_vid = item
                    .get("lastVideoId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                last_vid.is_empty() || !watched.contains(last_vid)
            })
            .collect();
        lib.insert("externalContinueWatching".into(), filtered.into());
    }

    let progress_map = lib
        .get("progress")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let cleaned: serde_json::Map<String, Value> = progress_map
        .into_iter()
        .filter(|(_, entry)| {
            let last_vid = entry
                .get("lastVideoId")
                .and_then(Value::as_str)
                .unwrap_or("");
            last_vid.is_empty() || !watched.contains(last_vid)
        })
        .collect();

    let progress_json = serde_json::to_string(&cleaned).unwrap_or_else(|_| "{}".to_string());
    let cw = build_continue_watching_from_progress_json(&progress_json)
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| Value::Array(Vec::new()));

    lib.insert("progress".into(), Value::Object(cleaned));
    lib.insert("continueWatching".into(), cw);

    serde_json::to_string(&Value::Object(lib)).ok()
}

pub(crate) fn merge_progress_meta_json(
    incoming_meta_json: &str,
    existing_meta_json: &str,
) -> String {
    let incoming: Value = serde_json::from_str(incoming_meta_json).unwrap_or(json!({}));
    let existing: Value = serde_json::from_str(existing_meta_json).unwrap_or(json!({}));

    let pick = |key: &str| -> Value {
        incoming
            .get(key)
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| existing.get(key).cloned())
            .unwrap_or(Value::Null)
    };

    let mut merged = incoming.clone();
    if let Some(obj) = merged.as_object_mut() {
        obj.insert("poster".into(), pick("poster"));
        obj.insert("background".into(), pick("background"));
        obj.insert("logo".into(), pick("logo"));
    }
    serde_json::to_string(&merged).unwrap_or_else(|_| incoming_meta_json.to_string())
}
