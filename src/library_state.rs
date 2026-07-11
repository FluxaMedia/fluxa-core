use serde_json::{json, Value};

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn number(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn library_item_from_meta(meta: &Value, state: Value, last_watched: Option<&str>) -> Value {
    let mut item = json!({
        "_id": text(meta, "id").unwrap_or(""),
        "name": text(meta, "name").unwrap_or(""),
        "type": text(meta, "type").unwrap_or(""),
        "poster": meta.get("poster").cloned().unwrap_or(Value::Null),
        "background": meta.get("background").cloned().unwrap_or(Value::Null),
        "logo": meta.get("logo").cloned().unwrap_or(Value::Null),
        "state": state
    });
    if let Some(last_watched) = last_watched {
        item["lastWatched"] = Value::String(last_watched.to_string());
    }
    item
}

pub(crate) fn playback_progress_item_json(
    meta_json: &str,
    time_offset: i64,
    duration: i64,
    now_utc: &str,
) -> Option<String> {
    let meta: Value = serde_json::from_str(meta_json).ok()?;
    let item = library_item_from_meta(
        &meta,
        json!({
            "lastWatched": now_utc,
            "timeOffset": time_offset,
            "duration": duration
        }),
        None,
    );
    serde_json::to_string(&item).ok()
}

pub(crate) fn clear_playback_progress_item_json(meta_json: &str) -> Option<String> {
    let meta: Value = serde_json::from_str(meta_json).ok()?;
    let item = library_item_from_meta(
        &meta,
        json!({
            "lastWatched": Value::Null,
            "timeOffset": 0,
            "duration": 0,
            "videoId": Value::Null,
            "timesWatched": 0,
            "flaggedWatched": 0
        }),
        None,
    );
    serde_json::to_string(&item).ok()
}

pub(crate) fn watched_state_items_json(
    meta_json: &str,
    episodes_json: &str,
    watched: bool,
    watched_at: Option<&str>,
) -> Option<String> {
    let meta: Value = serde_json::from_str(meta_json).ok()?;
    let episodes: Vec<Value> = serde_json::from_str(episodes_json).unwrap_or_default();
    let watched_value = if watched { 1 } else { 0 };
    let watched_at_value = watched_at
        .map(|value| Value::String(value.to_string()))
        .unwrap_or(Value::Null);
    let items = if text(&meta, "type") == Some("series") && !episodes.is_empty() {
        episodes
            .iter()
            .map(|episode| {
                json!({
                    "_id": text(episode, "id").unwrap_or(""),
                    "name": text(episode, "name").or_else(|| text(&meta, "name")).unwrap_or(""),
                    "type": "series",
                    "poster": episode.get("thumbnail").cloned().unwrap_or(Value::Null),
                    "background": meta.get("background").cloned().unwrap_or(Value::Null),
                    "logo": meta.get("logo").cloned().unwrap_or(Value::Null),
                    "state": {
                        "lastWatched": watched_at_value,
                        "timeOffset": 0,
                        "duration": 0,
                        "videoId": text(episode, "id").unwrap_or(""),
                        "timesWatched": watched_value,
                        "flaggedWatched": watched_value
                    },
                    "lastWatched": watched_at_value
                })
            })
            .collect::<Vec<_>>()
    } else {
        vec![library_item_from_meta(
            &meta,
            json!({
                "lastWatched": watched_at_value,
                "timeOffset": 0,
                "duration": 0,
                "videoId": Value::Null,
                "timesWatched": watched_value,
                "flaggedWatched": watched_value
            }),
            watched_at,
        )]
    };
    serde_json::to_string(&items).ok()
}

pub(crate) fn library_continue_watching_items_json(items_json: &str) -> Option<String> {
    let mut items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    items.retain(|item| {
        let state = item.get("state").unwrap_or(&Value::Null);
        !state.is_null()
            && number(state, "timeOffset").unwrap_or(0) > 0
            && number(state, "flaggedWatched").unwrap_or(0) == 0
    });
    items.sort_by(|a, b| {
        let a = a
            .get("state")
            .and_then(|state| text(state, "lastWatched"))
            .unwrap_or("");
        let b = b
            .get("state")
            .and_then(|state| text(state, "lastWatched"))
            .unwrap_or("");
        b.cmp(a)
    });
    let metas = items
        .into_iter()
        .map(|item| {
            let state = item.get("state").unwrap_or(&Value::Null);
            json!({
                "id": text(&item, "_id").unwrap_or(""),
                "name": text(&item, "name").unwrap_or(""),
                "type": text(&item, "type").unwrap_or(""),
                "poster": item.get("poster").cloned().unwrap_or(Value::Null),
                "background": item.get("background").cloned().unwrap_or(Value::Null),
                "logo": item.get("logo").cloned().unwrap_or(Value::Null),
                "description": Value::Null,
                "timeOffset": number(state, "timeOffset"),
                "duration": number(state, "duration"),
                "lastVideoId": text(state, "videoId"),
                "reason": "stremio"
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&metas).ok()
}

pub(crate) fn filter_home_continue_watching_json(
    items_json: &str,
    trakt_watched_json: &str,
) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let trakt: Value = serde_json::from_str(trakt_watched_json).unwrap_or(Value::Null);

    let movie_keys: std::collections::HashSet<&str> = trakt
        .get("movieKeys")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let episode_keys: std::collections::HashSet<&str> = trakt
        .get("episodeKeys")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let filtered: Vec<&Value> = items
        .iter()
        .filter(|item| {
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
            let last_video_id = item
                .get("lastVideoId")
                .and_then(Value::as_str)
                .unwrap_or("");
            let time_offset = item.get("timeOffset").and_then(Value::as_i64).unwrap_or(0);
            let duration = item.get("duration").and_then(Value::as_i64).unwrap_or(0);
            let is_series = matches!(item_type, "series" | "tv" | "anime");
            let is_up_next =
                is_series && !last_video_id.is_empty() && time_offset <= 0 && duration <= 0;
            let has_progress = time_offset > 0 && duration > 0;
            if !is_up_next && !has_progress {
                return false;
            }
            let watched_keys = crate::content_identity::content_watched_keys_value(item);
            if item_type == "movie"
                && !movie_keys.is_empty()
                && watched_keys.iter().any(|k| movie_keys.contains(k.as_str()))
            {
                return false;
            }
            if is_series && !episode_keys.is_empty() && !last_video_id.is_empty() {
                if let Some((_, season, episode)) =
                    crate::content_identity::parse_episode_locator(last_video_id)
                {
                    if watched_keys.iter().any(|k| {
                        let candidate = format!("{k}:{season}:{episode}");
                        episode_keys.contains(candidate.as_str())
                    }) {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    serde_json::to_string(&filtered).ok()
}

pub(crate) fn watched_video_ids_json(items_json: &str, imdb_id: &str) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let ids = items
        .iter()
        .filter(|item| {
            text(item, "_id").is_some_and(|id| id.starts_with(imdb_id))
                && item
                    .get("state")
                    .and_then(|state| number(state, "flaggedWatched"))
                    == Some(1)
        })
        .filter_map(|item| text(item, "_id").map(str::to_string))
        .collect::<Vec<_>>();
    serde_json::to_string(&ids).ok()
}

pub(crate) fn normalize_library_document_json(json: &str) -> String {
    let mut lib: serde_json::Map<String, Value> = serde_json::from_str(json).unwrap_or_default();
    lib.insert("schemaVersion".to_string(), json!(2));
    if !lib.get("watchlist").map(Value::is_array).unwrap_or(false) {
        lib.insert("watchlist".to_string(), json!([]));
    }
    if !lib.get("history").map(Value::is_array).unwrap_or(false) {
        lib.insert("history".to_string(), json!([]));
    }
    if !lib
        .get("continueWatching")
        .map(Value::is_array)
        .unwrap_or(false)
    {
        lib.insert("continueWatching".to_string(), json!([]));
    }
    if !lib
        .get("progress")
        .map(|v| v.is_object() && !v.is_array())
        .unwrap_or(false)
    {
        lib.insert("progress".to_string(), json!({}));
    }
    if !lib
        .get("watched")
        .map(|v| v.is_object() && !v.is_array())
        .unwrap_or(false)
    {
        lib.insert("watched".to_string(), json!({}));
    }
    if !lib.get("dropped").map(Value::is_array).unwrap_or(false) {
        lib.insert("dropped".to_string(), json!([]));
    }
    if !lib.get("completed").map(Value::is_array).unwrap_or(false) {
        lib.insert("completed".to_string(), json!([]));
    }
    serde_json::to_string(&Value::Object(lib)).unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn is_up_next_continue_watching_item_json(item_json: &str) -> bool {
    let item: Value = serde_json::from_str(item_json).unwrap_or(Value::Null);
    is_up_next_item(&item)
}

fn is_up_next_item(item: &Value) -> bool {
    let offset = item
        .get("timeOffset")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let duration = item.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
    if duration <= 0.0 {
        return offset <= 1.0;
    }
    let progress = offset / duration;
    !(0.005..0.995).contains(&progress)
}

pub(crate) fn build_continue_watching_from_progress_json(progress_json: &str) -> Option<String> {
    let progress: serde_json::Map<String, Value> = serde_json::from_str(progress_json).ok()?;
    let mut items: Vec<Value> = progress.values()
        .filter_map(|entry| {
            let offset = entry.get("timeOffset").and_then(Value::as_f64).unwrap_or(0.0);
            let duration = entry.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
            let has_video_id = entry.get("lastVideoId").and_then(Value::as_str).filter(|s| !s.is_empty()).is_some();
            // Include: items with real progress OR up-next entries (offset=0 but has lastVideoId)
            let include = (offset > 0.0 && duration > 0.0 && offset / duration < 0.95)
                || (offset == 0.0 && has_video_id);
            if !include { return None; }
            let meta = entry.get("meta")?;
            let id = meta.get("id").and_then(Value::as_str).unwrap_or("");
            if id.is_empty() { return None; }
            Some(json!({
                "id": id,
                "name": meta.get("name").and_then(Value::as_str).unwrap_or(""),
                "type": meta.get("type").and_then(Value::as_str).unwrap_or(""),
                "poster": meta.get("poster").cloned().unwrap_or(Value::Null),
                "background": meta.get("background").cloned().unwrap_or(Value::Null),
                "logo": meta.get("logo").cloned().unwrap_or(Value::Null),
                "timeOffset": offset as i64,
                "duration": duration as i64,
                "lastVideoId": entry.get("lastVideoId").cloned().unwrap_or(Value::Null),
                "lastEpisodeName": entry.get("lastEpisodeName").cloned().unwrap_or(Value::Null),
                "lastEpisodeSeason": entry.get("lastEpisodeSeason").cloned().unwrap_or(Value::Null),
                "lastEpisodeNumber": entry.get("lastEpisodeNumber").cloned().unwrap_or(Value::Null),
                "lastEpisodeThumbnail": entry.get("lastEpisodeThumbnail").cloned().unwrap_or(Value::Null),
                "lastStreamUrl": entry.get("lastStreamUrl").cloned().unwrap_or(Value::Null),
                "lastStreamTitle": entry.get("lastStreamTitle").cloned().unwrap_or(Value::Null),
                "lastStream": entry.get("lastStream").cloned().unwrap_or(Value::Null),
                "continueWatchingBadge": entry.get("continueWatchingBadge").cloned().unwrap_or(Value::Null),
                "continueWatchingEpisodeResolved": entry.get("continueWatchingEpisodeResolved").cloned().unwrap_or(Value::Null),
                "savedAt": entry.get("savedAt").cloned().unwrap_or(Value::Null),
                "source": entry.get("source").cloned().unwrap_or(Value::Null),
            }))
        })
        .collect();
    items.sort_by(|a, b| {
        let a = a.get("savedAt").and_then(Value::as_str).unwrap_or("");
        let b = b.get("savedAt").and_then(Value::as_str).unwrap_or("");
        b.cmp(a)
    });
    serde_json::to_string(&items).ok()
}

pub(crate) fn compute_continue_watching_badges_json(
    candidates_json: &str,
    videos_by_series_json: &str,
    last_watched_json: &str,
    now_ms: i64,
) -> Option<String> {
    let mut by_id: std::collections::HashMap<String, Value> = {
        let items: Vec<Value> = serde_json::from_str(candidates_json).unwrap_or_default();
        items
            .into_iter()
            .filter_map(|item| {
                let id = item
                    .get("id")
                    .or_else(|| item.get("_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)?;
                Some((id, item))
            })
            .collect()
    };
    let videos_by_series: serde_json::Map<String, Value> =
        serde_json::from_str(videos_by_series_json).unwrap_or_default();
    let last_watched: serde_json::Map<String, Value> =
        serde_json::from_str(last_watched_json).unwrap_or_default();

    // Track which IDs came from the real CW lists vs only from lastWatchedEpisodes.
    // Candidates added only from lastWatchedEpisodes are removed when no video data
    // is available to confirm a next episode exists, preventing phantom CW entries.
    let cw_list_ids: std::collections::HashSet<String> = by_id.keys().cloned().collect();

    seed_candidates_from_last_watched(&mut by_id, &last_watched);

    let mut finished_series: Vec<String> = Vec::new();
    for (series_id, candidate) in by_id.iter_mut() {
        let next =
            match next_episode_for_candidate(series_id, candidate, &videos_by_series, &cw_list_ids)
            {
                NextEpisodeOutcome::Skip => continue,
                NextEpisodeOutcome::MarkFinished => {
                    finished_series.push(series_id.clone());
                    continue;
                }
                NextEpisodeOutcome::Found(next) => next,
            };
        let videos = videos_by_series
            .get(series_id)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        apply_next_episode_badge(series_id, candidate, &next, videos, now_ms);
    }

    for id in &finished_series {
        by_id.remove(id);
    }
    let mut result: Vec<Value> = by_id.into_values().collect();
    result.sort_by(|a, b| {
        let a_new = a.get("continueWatchingBadge").and_then(Value::as_str) == Some("newEpisode");
        let b_new = b.get("continueWatchingBadge").and_then(Value::as_str) == Some("newEpisode");
        if a_new != b_new {
            return if a_new {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let a_time = a
            .get("savedAt")
            .or_else(|| a.get("newEpisodeReleasedAt"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let b_time = b
            .get("savedAt")
            .or_else(|| b.get("newEpisodeReleasedAt"))
            .and_then(Value::as_str)
            .unwrap_or("");
        b_time.cmp(a_time)
    });
    serde_json::to_string(&result).ok()
}

// Adds a synthetic CW candidate for any series that only has a lastWatchedEpisodes
// record (no real continue-watching entry yet) so its next-episode badge still gets
// computed below; `by_id.entry(...).or_insert_with` leaves real entries untouched.
fn seed_candidates_from_last_watched(
    by_id: &mut std::collections::HashMap<String, Value>,
    last_watched: &serde_json::Map<String, Value>,
) {
    for (series_id, raw) in last_watched {
        let meta = match raw.get("meta") {
            Some(m) if m.get("type").and_then(Value::as_str) == Some("series") => m,
            _ => continue,
        };
        let record = raw;
        by_id.entry(series_id.clone()).or_insert_with(|| json!({
            "id": series_id,
            "_id": series_id,
            "type": "series",
            "name": meta.get("name").cloned().unwrap_or(Value::Null),
            "poster": meta.get("poster").cloned().unwrap_or(Value::Null),
            "background": meta.get("background").cloned().unwrap_or(Value::Null),
            "logo": meta.get("logo").cloned().unwrap_or(Value::Null),
            "lastVideoId": record.get("lastVideoId").cloned().unwrap_or(Value::Null),
            "lastEpisodeName": record.get("lastEpisodeName").cloned().unwrap_or(Value::Null),
            "lastEpisodeSeason": record.get("lastEpisodeSeason").cloned().unwrap_or(Value::Null),
            "lastEpisodeNumber": record.get("lastEpisodeNumber").cloned().unwrap_or(Value::Null),
            "lastEpisodeThumbnail": record.get("lastEpisodeThumbnail").cloned().unwrap_or(Value::Null),
            "timeOffset": 1,
            "duration": 99999,
            "savedAt": record.get("watchedAt").cloned().unwrap_or(Value::Null),
        }));
    }
}

enum NextEpisodeOutcome {
    Skip,
    MarkFinished,
    Found(Value),
}

// Decides what's next for one candidate: skip it untouched, mark its series as
// finished (to be dropped from the result), or hand back the episode to advance to.
fn next_episode_for_candidate(
    series_id: &str,
    candidate: &Value,
    videos_by_series: &serde_json::Map<String, Value>,
    cw_list_ids: &std::collections::HashSet<String>,
) -> NextEpisodeOutcome {
    if candidate.get("type").and_then(Value::as_str) != Some("series") {
        return NextEpisodeOutcome::Skip;
    }
    if !is_up_next_item(candidate) {
        return NextEpisodeOutcome::Skip;
    }
    let Some(season) = candidate.get("lastEpisodeSeason").and_then(Value::as_i64) else {
        return NextEpisodeOutcome::Skip;
    };
    let Some(episode) = candidate.get("lastEpisodeNumber").and_then(Value::as_i64) else {
        return NextEpisodeOutcome::Skip;
    };
    let Some(videos) = videos_by_series.get(series_id).and_then(Value::as_array) else {
        // No video data available. If this entry exists only because of
        // lastWatchedEpisodes (not from any real CW list), conservatively
        // remove it — we cannot confirm a next episode exists. It will
        // reappear on the next home load once the addon responds.
        return if !cw_list_ids.contains(series_id) {
            NextEpisodeOutcome::MarkFinished
        } else {
            NextEpisodeOutcome::Skip
        };
    };
    let stored_badge = candidate
        .get("continueWatchingBadge")
        .and_then(Value::as_str);
    if stored_badge == Some("upNext")
        && candidate
            .get("continueWatchingEpisodeResolved")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return NextEpisodeOutcome::Skip;
    }
    let stored_video_id = candidate
        .get("lastVideoId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // When the stored badge is scheduledEpisode, lastEpisodeNumber already points to the
    // scheduled episode itself. Re-check that same episode rather than advancing past it.
    let next = if stored_badge == Some("scheduledEpisode") {
        videos
            .iter()
            .find(|v| {
                let vid = v
                    .get("id")
                    .or_else(|| v.get("_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                vid == stored_video_id
            })
            .cloned()
            .or_else(|| first_episode_after(videos, season, episode))
    } else {
        first_episode_after(videos, season, episode)
    };

    // No next episode and we have real video data — the series is fully watched.
    // Remove it from Continue Watching instead of leaving a zombie entry.
    match next {
        Some(v) => NextEpisodeOutcome::Found(v),
        None => NextEpisodeOutcome::MarkFinished,
    }
}

// Computes the badge (upNext / newEpisode / scheduledEpisode) for advancing `candidate`
// to `next`, and rewrites `candidate` in place to point at that episode.
fn apply_next_episode_badge(
    series_id: &str,
    candidate: &mut Value,
    next: &Value,
    videos: &[Value],
    now_ms: i64,
) {
    let existing_video_id = candidate
        .get("lastVideoId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let next_id = next
        .get("id")
        .or_else(|| next.get("_id"))
        .and_then(Value::as_str)
        .unwrap_or(&existing_video_id)
        .to_string();
    if !is_up_next_item(candidate) && existing_video_id != next_id {
        return;
    }
    let is_new_target = existing_video_id != next_id;
    let is_released = is_episode_released(next, now_ms);
    let existing_badge = if !is_new_target {
        candidate
            .get("continueWatchingBadge")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        None
    };

    let badge = if !is_released {
        "scheduledEpisode"
    } else if existing_badge.as_deref() == Some("scheduledEpisode") {
        "newEpisode"
    } else if let Some(b) = existing_badge.as_deref() {
        b
    } else {
        let watched_at = candidate
            .get("savedAt")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(now_ms);
        let next_released_at = next
            .get("released")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(0);
        let was_released_when_watched =
            next.get("released").is_none() || next_released_at <= watched_at;
        if was_released_when_watched {
            "upNext"
        } else {
            "newEpisode"
        }
    }
    .to_string();

    let released_str = next
        .get("released")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let saved_at_new = if is_new_target && badge == "newEpisode" {
        Value::String(chrono::Utc::now().to_rfc3339())
    } else {
        candidate.get("savedAt").cloned().unwrap_or(Value::Null)
    };

    let unwatched_ahead = if badge == "scheduledEpisode" {
        0
    } else {
        let next_season = next.get("season").and_then(Value::as_i64).unwrap_or(0);
        let next_episode = next
            .get("episode")
            .or_else(|| next.get("number"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        count_released_after(videos, next_season, next_episode, now_ms)
    };

    *candidate = json!({
        "id": series_id,
        "_id": series_id,
        "type": "series",
        "name": candidate.get("name").cloned().unwrap_or(Value::Null),
        "poster": candidate.get("poster").cloned().unwrap_or(Value::Null),
        "background": candidate.get("background").cloned().unwrap_or(Value::Null),
        "logo": candidate.get("logo").cloned().unwrap_or(Value::Null),
        "timeOffset": 1,
        "duration": 99999,
        "lastVideoId": next_id,
        "lastEpisodeName": next.get("name").or_else(|| next.get("title")).cloned().unwrap_or(Value::Null),
        "lastEpisodeSeason": next.get("season").cloned().unwrap_or(Value::Null),
        "lastEpisodeNumber": next.get("episode").or_else(|| next.get("number")).cloned().unwrap_or(Value::Null),
        "lastEpisodeThumbnail": next.get("thumbnail")
            .filter(|v| v.as_str().map_or(!v.is_null(), |s| !s.trim().is_empty()))
            .cloned()
            .or_else(|| if !is_new_target {
                candidate.get("lastEpisodeThumbnail").cloned().filter(|v| v.as_str().map_or(!v.is_null(), |s| !s.trim().is_empty()))
            } else {
                None
            })
            .unwrap_or(Value::Null),
        "continueWatchingBadge": badge,
        "newEpisodeReleasedAt": released_str,
        "savedAt": saved_at_new,
        "unwatchedAhead": unwatched_ahead,
    });
}

fn count_released_after(videos: &[Value], season: i64, episode: i64, now_ms: i64) -> i64 {
    videos
        .iter()
        .filter(|v| {
            let vs = v.get("season").and_then(Value::as_i64).unwrap_or(0);
            let ve = v
                .get("episode")
                .or_else(|| v.get("number"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            (vs > season || (vs == season && ve > episode)) && is_episode_released(v, now_ms)
        })
        .count() as i64
}

fn first_episode_after(videos: &[Value], season: i64, episode: i64) -> Option<Value> {
    let mut candidates: Vec<&Value> = videos
        .iter()
        .filter(|v| {
            let vs = v.get("season").and_then(Value::as_i64).unwrap_or(0);
            let ve = v
                .get("episode")
                .or_else(|| v.get("number"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            vs > season || (vs == season && ve > episode)
        })
        .collect();
    candidates.sort_by(|a, b| {
        let as_ = a.get("season").and_then(Value::as_i64).unwrap_or(0);
        let bs = b.get("season").and_then(Value::as_i64).unwrap_or(0);
        if as_ != bs {
            return as_.cmp(&bs);
        }
        let ae = a
            .get("episode")
            .or_else(|| a.get("number"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let be = b
            .get("episode")
            .or_else(|| b.get("number"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        ae.cmp(&be)
    });
    candidates.first().map(|v| (*v).clone())
}

pub(crate) fn is_episode_released(video: &Value, now_ms: i64) -> bool {
    let released = match video.get("released").and_then(Value::as_str) {
        Some(s) => s,
        None => return true,
    };
    match chrono::DateTime::parse_from_rfc3339(released) {
        Ok(dt) => dt.timestamp_millis() <= now_ms,
        Err(_) => true,
    }
}

/// Given a library JSON and a set of just-watched video IDs, update `lastWatchedEpisodes`.
/// Returns the updated library as JSON.
pub(crate) fn remember_last_watched_episodes_json(
    lib_json: &str,
    watched_ids_json: &str,
) -> String {
    let mut lib: Value = serde_json::from_str(lib_json).unwrap_or(json!({}));
    let watched_ids: std::collections::HashSet<String> = serde_json::from_str(watched_ids_json)
        .ok()
        .and_then(|v: Value| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default();
    let progress = lib
        .get("progress")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut last_watched = lib
        .get("lastWatchedEpisodes")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (series_id, raw) in &progress {
        let video_id = raw.get("lastVideoId").and_then(Value::as_str).unwrap_or("");
        if video_id.is_empty() || !watched_ids.contains(video_id) {
            continue;
        }
        let meta = match raw.get("meta") {
            Some(m) if m.get("type").and_then(Value::as_str) == Some("series") => m,
            _ => continue,
        };
        last_watched.insert(series_id.clone(), json!({
            "meta": meta,
            "lastVideoId": video_id,
            "lastEpisodeName": raw.get("lastEpisodeName").cloned().unwrap_or(Value::Null),
            "lastEpisodeSeason": raw.get("lastEpisodeSeason").cloned().unwrap_or(Value::Null),
            "lastEpisodeNumber": raw.get("lastEpisodeNumber").cloned().unwrap_or(Value::Null),
            "lastEpisodeThumbnail": raw.get("lastEpisodeThumbnail").cloned().unwrap_or(Value::Null),
            "watchedAt": chrono::Utc::now().to_rfc3339(),
        }));
    }
    if let Some(obj) = lib.as_object_mut() {
        obj.insert(
            "lastWatchedEpisodes".to_string(),
            Value::Object(last_watched),
        );
    }
    serde_json::to_string(&lib).unwrap_or_else(|_| lib_json.to_string())
}

/// Returns the next episode after (current_season, current_episode).
/// If released_only is true, episodes whose `released` date is in the future
/// (relative to now_ms) are excluded.
pub(crate) fn resolve_next_episode_json(
    videos_json: &str,
    current_season: i64,
    current_episode: i64,
    now_ms: i64,
    released_only: bool,
) -> Option<String> {
    let videos: Vec<Value> = serde_json::from_str(videos_json).ok()?;
    let filtered: Vec<&Value> = videos
        .iter()
        .filter(|v| !released_only || is_episode_released(v, now_ms))
        .collect();
    let next = first_episode_after(
        &filtered.into_iter().cloned().collect::<Vec<_>>(),
        current_season,
        current_episode,
    )?;
    serde_json::to_string(&next).ok()
}

/// Formats a "S1:E2 Episode Name" line from the episode progress fields.
/// Falls back to parsing season/episode from lastVideoId when the explicit
/// season/episode numbers are absent.
pub(crate) fn format_episode_line_json(
    last_episode_name: Option<&str>,
    last_episode_season: Option<i64>,
    last_episode_number: Option<i64>,
    last_video_id: Option<&str>,
) -> String {
    let mut season = last_episode_season;
    let mut episode = last_episode_number;

    if season.is_none() || episode.is_none() {
        if let Some(id) = last_video_id.filter(|id| !id.is_empty()) {
            let parts: Vec<&str> = id.split(':').collect();
            if parts.len() >= 3 {
                if let (Ok(s), Ok(e)) = (
                    parts[parts.len() - 2].parse::<i64>(),
                    parts[parts.len() - 1].parse::<i64>(),
                ) {
                    if s > 0 && e > 0 {
                        if season.is_none() {
                            season = Some(s);
                        }
                        if episode.is_none() {
                            episode = Some(e);
                        }
                    }
                }
            }
        }
    }

    let code = match (season, episode) {
        (Some(s), Some(e)) => format!("S{s}:E{e}"),
        _ => String::new(),
    };
    let name = last_episode_name.map(str::trim).unwrap_or("").to_string();
    [code, name]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Selects the best artwork URL for a continue-watching card.
/// `artwork_preference` is "poster", "background", or "episode" (default).
/// `is_horizontal` controls whether the card layout is wide/horizontal.
pub(crate) fn select_continue_watching_artwork_json(
    item_json: &str,
    artwork_preference: &str,
    is_horizontal: bool,
) -> Option<String> {
    let item: Value = serde_json::from_str(item_json).ok()?;
    let str_field = |key: &str| -> Option<String> {
        item.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
    };

    let poster = str_field("poster");
    let background = str_field("background");
    let logo = str_field("logo");
    let thumbnail = str_field("lastEpisodeThumbnail");
    let cw_poster = str_field("continueWatchingPoster");
    let cw_background = str_field("continueWatchingBackground");

    let is_real_backdrop = background.as_deref().is_some_and(|bg| {
        (poster.as_deref() != Some(bg)) && !bg.to_lowercase().contains("/poster/")
    });
    let existing_backdrop = if is_real_backdrop {
        background.clone()
    } else {
        None
    };

    let result = if !is_horizontal {
        thumbnail
            .or(cw_poster)
            .or(poster)
            .or(cw_background)
            .or(background)
    } else {
        let content_type = item.get("type").and_then(Value::as_str).unwrap_or("");
        let is_series = matches!(content_type, "series" | "tv" | "anime");
        let _ = is_series;
        match artwork_preference {
            "poster" => poster.or(cw_background).or(existing_backdrop),
            "background" => existing_backdrop.or(cw_background).or(poster),
            _ => thumbnail
                .or(cw_background)
                .or(existing_backdrop)
                .or(background)
                .or(logo)
                .or(poster),
        }
    };

    result
}

/// Batched form of select_continue_watching_artwork_json + format_episode_line_json for
/// a whole Continue Watching row at once — each card used to call both over IPC
/// individually, which meant one IPC round trip per card on every Home load.
pub(crate) fn continue_watching_card_fields_json(
    items_json: &str,
    artwork_preference: &str,
    is_horizontal: bool,
) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let fields: Vec<Value> = items
        .iter()
        .map(|item| {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let artwork = select_continue_watching_artwork_json(
                &item.to_string(),
                artwork_preference,
                is_horizontal,
            );
            let episode_line = format_episode_line_json(
                item.get("lastEpisodeName").and_then(Value::as_str),
                item.get("lastEpisodeSeason").and_then(Value::as_i64),
                item.get("lastEpisodeNumber").and_then(Value::as_i64),
                item.get("lastVideoId").and_then(Value::as_str),
            );
            json!({ "id": id, "artwork": artwork, "episodeLine": episode_line })
        })
        .collect();
    serde_json::to_string(&fields).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn watched_state_items_build_series_episode_payloads() {
        let items = watched_state_items_json(
            r#"{"id":"tt1","name":"Show","type":"series","poster":null,"background":"bg","logo":"logo"}"#,
            r#"[{"id":"tt1:1:2","name":null,"season":1,"number":2,"released":null,"thumbnail":"ep.jpg"}]"#,
            true,
            Some("2026-01-01T00:00:00.000Z"),
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("items");

        assert_eq!(
            items
                .get(0)
                .and_then(|item| item.get("_id"))
                .and_then(Value::as_str),
            Some("tt1:1:2")
        );
        assert_eq!(
            items
                .get(0)
                .and_then(|item| item.get("state"))
                .and_then(|state| state.get("flaggedWatched"))
                .and_then(Value::as_i64),
            Some(1)
        );
    }

    #[test]
    fn continue_watching_badges_advance_to_next_episode_and_drop_unconfirmed_finished_series() {
        let candidates = json!([{
            "id": "s1",
            "_id": "s1",
            "type": "series",
            "lastVideoId": "s1:1:2",
            "lastEpisodeSeason": 1,
            "lastEpisodeNumber": 2,
            "timeOffset": 1,
            "duration": 99999,
            "savedAt": "2020-02-01T00:00:00Z",
        }]);
        let videos_by_series = json!({
            "s1": [
                { "id": "s1:1:2", "season": 1, "episode": 2, "released": "2020-01-01T00:00:00Z" },
                { "id": "s1:1:3", "season": 1, "episode": 3, "released": "2020-01-08T00:00:00Z" },
            ],
        });
        // s3 exists only via lastWatchedEpisodes (no real CW entry, no video data) —
        // it should be dropped rather than left as a zombie entry.
        let last_watched = json!({
            "s3": {
                "meta": { "type": "series", "name": "Only From Last Watched" },
                "lastVideoId": "s3:1:1",
                "lastEpisodeSeason": 1,
                "lastEpisodeNumber": 1,
                "watchedAt": "2020-01-01T00:00:00Z",
            },
        });
        let now_ms = chrono::DateTime::parse_from_rfc3339("2021-01-01T00:00:00Z")
            .unwrap()
            .timestamp_millis();

        let result = compute_continue_watching_badges_json(
            &candidates.to_string(),
            &videos_by_series.to_string(),
            &last_watched.to_string(),
            now_ms,
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("badges");
        let result = result.as_array().unwrap();

        assert_eq!(
            result.len(),
            1,
            "s3 (no video data, not from a real CW list) should be dropped"
        );
        assert_eq!(result[0]["id"], "s1");
        assert_eq!(result[0]["lastVideoId"], "s1:1:3");
        assert_eq!(result[0]["continueWatchingBadge"], "upNext");
    }

    #[test]
    fn continue_watching_badges_do_not_double_advance_resolved_up_next_entries() {
        let candidates = json!([{
            "id": "s1",
            "_id": "s1",
            "type": "series",
            "lastVideoId": "s1:2:3",
            "lastEpisodeSeason": 2,
            "lastEpisodeNumber": 3,
            "timeOffset": 1,
            "duration": 99999,
            "continueWatchingBadge": "upNext",
            "continueWatchingEpisodeResolved": true,
            "savedAt": "2020-02-01T00:00:00Z",
        }]);
        let videos_by_series = json!({
            "s1": [
                { "id": "s1:2:3", "season": 2, "episode": 3, "released": "2020-01-01T00:00:00Z" },
                { "id": "s1:2:4", "season": 2, "episode": 4, "released": "2020-01-08T00:00:00Z" },
            ],
        });
        let now_ms = chrono::DateTime::parse_from_rfc3339("2021-01-01T00:00:00Z")
            .unwrap()
            .timestamp_millis();

        let result = compute_continue_watching_badges_json(
            &candidates.to_string(),
            &videos_by_series.to_string(),
            "{}",
            now_ms,
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("badges");
        let result = result.as_array().unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["lastVideoId"], "s1:2:3");
        assert_eq!(result[0]["lastEpisodeNumber"], 3);
        assert_eq!(result[0]["continueWatchingBadge"], "upNext");
    }

    #[test]
    fn continue_watching_badges_count_only_released_episodes_ahead() {
        let candidates = json!([{
            "id": "s1",
            "_id": "s1",
            "type": "series",
            "lastVideoId": "s1:1:2",
            "lastEpisodeSeason": 1,
            "lastEpisodeNumber": 2,
            "timeOffset": 1,
            "duration": 99999,
            "savedAt": "2020-02-01T00:00:00Z",
        }]);
        let videos_by_series = json!({
            "s1": [
                { "id": "s1:1:2", "season": 1, "episode": 2, "released": "2020-01-01T00:00:00Z" },
                { "id": "s1:1:3", "season": 1, "episode": 3, "released": "2020-01-08T00:00:00Z" },
                { "id": "s1:1:4", "season": 1, "episode": 4, "released": "2020-01-15T00:00:00Z" },
                { "id": "s1:1:5", "season": 1, "episode": 5, "released": "2099-01-01T00:00:00Z" },
            ],
        });
        let now_ms = chrono::DateTime::parse_from_rfc3339("2021-01-01T00:00:00Z")
            .unwrap()
            .timestamp_millis();

        let result = compute_continue_watching_badges_json(
            &candidates.to_string(),
            &videos_by_series.to_string(),
            "{}",
            now_ms,
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("badges");
        let result = result.as_array().unwrap();

        assert_eq!(result[0]["lastVideoId"], "s1:1:3");
        assert_eq!(result[0]["unwatchedAhead"], 1);
    }
}
