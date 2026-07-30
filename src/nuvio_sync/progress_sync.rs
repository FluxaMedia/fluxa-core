use super::helpers::{iso_from_ms, parse, str_field};
use serde_json::{json, Map, Value};

const RESOLVED_LOW_RATIO: f64 = 0.005;
const RESOLVED_HIGH_RATIO: f64 = 0.995;
const RESOLVED_MAX_POSITION_MS: f64 = 1000.0;
pub(crate) fn library_to_watchlist_json(args_json: &str) -> Option<String> {
    let args = parse(args_json)?;
    let library = args.get("library")?.as_array()?.clone();
    let watchlist: Vec<Value> = library
        .iter()
        .map(|item| {
            let mut out = Map::new();
            out.insert(
                "id".into(),
                item.get("content_id").cloned().unwrap_or(Value::Null),
            );
            out.insert(
                "name".into(),
                item.get("name").cloned().unwrap_or(Value::Null),
            );
            out.insert(
                "type".into(),
                item.get("content_type").cloned().unwrap_or(Value::Null),
            );
            for (dst, src) in [
                ("poster", "poster"),
                ("background", "background"),
                ("description", "description"),
                ("releaseInfo", "release_info"),
                ("imdbRating", "imdb_rating"),
            ] {
                if let Some(v) = item.get(src).filter(|v| !v.is_null()) {
                    out.insert(dst.into(), v.clone());
                }
            }
            if let Some(genres) = item.get("genres").and_then(Value::as_array) {
                if !genres.is_empty() {
                    out.insert("genres".into(), Value::Array(genres.clone()));
                }
            }
            out.insert("inWatchlist".into(), Value::Bool(true));
            Value::Object(out)
        })
        .collect();
    Some(Value::Array(watchlist).to_string())
}

pub(crate) fn progress_meta_needs_json(args_json: &str) -> Option<String> {
    let args = parse(args_json)?;
    let watch_progress = args.get("watchProgress")?.as_array()?.clone();
    let library = args
        .get("library")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let library_ids: Vec<&Value> = library.iter().filter_map(|i| i.get("content_id")).collect();

    let needs: Vec<Value> = watch_progress
        .iter()
        .filter(|e| {
            let is_series = str_field(e, "content_type") == Some("series");
            let in_library = e
                .get("content_id")
                .map(|id| library_ids.contains(&id))
                .unwrap_or(false);
            is_series || !in_library
        })
        .map(|e| {
            json!({
                "contentId": e.get("content_id").cloned().unwrap_or(Value::Null),
                "contentType": e.get("content_type").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    Some(Value::Array(needs).to_string())
}

fn progress_entry(entry: &Value, lib_item: Option<&Value>, addon_meta: Option<&Value>) -> Value {
    let position = entry.get("position").and_then(Value::as_f64).unwrap_or(0.0);
    let duration = entry.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
    let ratio = if duration > 0.0 {
        position / duration
    } else {
        0.0
    };
    let is_resolved_up_next = if duration <= 0.0 {
        position <= RESOLVED_MAX_POSITION_MS
    } else {
        ratio < RESOLVED_LOW_RATIO || ratio >= RESOLVED_HIGH_RATIO
    };

    let season = entry.get("season").filter(|v| !v.is_null());
    let episode = entry.get("episode").filter(|v| !v.is_null());
    let num_eq = |a: Option<&Value>, b: Option<&Value>| match (
        a.and_then(Value::as_f64),
        b.and_then(Value::as_f64),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    let ep_meta = match (
        season,
        episode,
        addon_meta
            .and_then(|m| m.get("videos"))
            .and_then(Value::as_array),
    ) {
        (Some(s), Some(e), Some(videos)) => videos
            .iter()
            .find(|v| num_eq(v.get("season"), Some(s)) && num_eq(v.get("episode"), Some(e))),
        _ => None,
    };

    let pick = |field: &str| -> Value {
        lib_item
            .and_then(|i| i.get(field))
            .filter(|v| !v.is_null())
            .or_else(|| {
                addon_meta
                    .and_then(|m| m.get(field))
                    .filter(|v| !v.is_null())
            })
            .cloned()
            .unwrap_or(Value::Null)
    };

    let mut out = Map::new();
    let mut meta = Map::new();
    meta.insert(
        "id".into(),
        entry.get("content_id").cloned().unwrap_or(Value::Null),
    );
    meta.insert(
        "type".into(),
        entry.get("content_type").cloned().unwrap_or(Value::Null),
    );
    meta.insert("name".into(), pick("name"));
    for field in ["poster", "background"] {
        let v = pick(field);
        if !v.is_null() {
            meta.insert(field.into(), v);
        }
    }
    out.insert("meta".into(), Value::Object(meta));
    out.insert(
        "timeOffset".into(),
        json!((position / 1000.0).round() as i64),
    );
    out.insert("duration".into(), json!((duration / 1000.0).round() as i64));
    out.insert(
        "lastVideoId".into(),
        entry.get("video_id").cloned().unwrap_or(Value::Null),
    );
    if let Some(s) = season {
        out.insert("lastEpisodeSeason".into(), s.clone());
    }
    if let Some(e) = episode {
        out.insert("lastEpisodeNumber".into(), e.clone());
    }
    if let Some(ep) = ep_meta {
        if let Some(title) = str_field(ep, "title").or_else(|| str_field(ep, "name")) {
            out.insert("lastEpisodeName".into(), Value::String(title.to_string()));
        }
        if let Some(thumb) = str_field(ep, "thumbnail") {
            out.insert(
                "lastEpisodeThumbnail".into(),
                Value::String(thumb.to_string()),
            );
        }
    }
    if is_resolved_up_next {
        out.insert(
            "continueWatchingBadge".into(),
            Value::String("upNext".into()),
        );
        out.insert("continueWatchingEpisodeResolved".into(), Value::Bool(true));
    }
    let last_watched = entry
        .get("last_watched")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    out.insert("savedAt".into(), Value::String(iso_from_ms(last_watched)));
    out.insert("source".into(), Value::String("nuvio".into()));
    Value::Object(out)
}

pub(crate) fn import_merge_plan_json(args_json: &str) -> Option<String> {
    let args = parse(args_json)?;
    let categories: Option<Vec<&str>> = args
        .get("categories")
        .filter(|v| !v.is_null())
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect());
    let dry_run = args.get("dryRun").and_then(Value::as_bool).unwrap_or(false);
    let wants = |c: &str| {
        categories
            .as_ref()
            .is_none_or(|cats| cats.iter().any(|x| *x == c))
    };
    let progress_count = args
        .get("watchProgress")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let watched_count = args
        .get("watchHistory")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let mut progress = args
        .get("progress")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut watched = args
        .get("watched")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let library = args
        .get("library")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let addon_metas = args
        .get("addonMetas")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut lib_by_id: Map<String, Value> = Map::new();
    for item in library {
        if let Some(id) = str_field(&item, "content_id") {
            lib_by_id.insert(id.to_string(), item.clone());
        }
    }

    let mut active_remote_ids: Vec<String> = Vec::new();
    if let Some(watch_progress) = args.get("watchProgress").and_then(Value::as_array) {
        let mut sorted = watch_progress.clone();
        sorted.sort_by_key(|e| e.get("last_watched").and_then(Value::as_i64).unwrap_or(0));
        for entry in &sorted {
            let Some(content_id) = str_field(entry, "content_id") else {
                continue;
            };
            progress.insert(
                content_id.to_string(),
                progress_entry(
                    entry,
                    lib_by_id.get(content_id),
                    addon_metas.get(content_id),
                ),
            );
            if let Some(video_id) = str_field(entry, "video_id") {
                active_remote_ids.push(video_id.to_string());
            }
            if let (Some(s), Some(e)) = (
                entry.get("season").and_then(Value::as_i64),
                entry.get("episode").and_then(Value::as_i64),
            ) {
                active_remote_ids.push(format!("{content_id}:{s}:{e}"));
            }
        }
    }

    if let Some(watch_history) = args.get("watchHistory").and_then(Value::as_array) {
        for item in watch_history {
            let Some(content_id) = str_field(item, "content_id") else {
                continue;
            };
            if str_field(item, "content_type") == Some("movie") {
                watched.insert(content_id.to_string(), Value::Bool(true));
            } else if let (Some(s), Some(e)) = (
                item.get("season").and_then(Value::as_i64),
                item.get("episode").and_then(Value::as_i64),
            ) {
                watched.insert(format!("{content_id}:{s}:{e}"), Value::Bool(true));
            }
        }
        for id in &active_remote_ids {
            watched.remove(id);
        }
    }

    let is_watched = |watched: &Map<String, Value>, key: &str| {
        watched.get(key).and_then(Value::as_bool).unwrap_or(false)
    };
    let mut to_remove: Vec<String> = Vec::new();
    for (content_id, entry) in &progress {
        let video_watched = str_field(entry, "lastVideoId")
            .map(|id| is_watched(&watched, id))
            .unwrap_or(false);
        let episode_watched = match (
            entry.get("lastEpisodeSeason").and_then(Value::as_i64),
            entry.get("lastEpisodeNumber").and_then(Value::as_i64),
        ) {
            (Some(s), Some(e)) => is_watched(&watched, &format!("{content_id}:{s}:{e}")),
            _ => false,
        };
        if video_watched || episode_watched {
            to_remove.push(content_id.clone());
        }
    }
    for id in to_remove {
        progress.remove(&id);
    }

    let progress_out = if wants("continueWatching") && !dry_run {
        Some(progress)
    } else {
        None
    };
    let watched_out = if wants("watched") && !dry_run {
        Some(watched)
    } else {
        None
    };

    Some(
        json!({
            "progress": progress_out,
            "progressCount": progress_count,
            "watched": watched_out,
            "watchedCount": watched_count,
        })
        .to_string(),
    )
}
