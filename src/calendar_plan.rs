use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) fn calendar_visibility_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let items = request.get("items")?.as_array()?;
    if request.get("showCompleted").and_then(Value::as_bool) == Some(true) {
        return serde_json::to_string(items).ok();
    }
    let completed = request.get("completedItems")?.as_array()?;
    let visible: Vec<&Value> = items
        .iter()
        .filter(|item| {
            let ids: Vec<&str> = ["contentId", "seriesId", "id"]
                .iter()
                .filter_map(|key| item.get(*key).and_then(Value::as_str))
                .collect();
            let name = item
                .get("title")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            !completed.iter().any(|entry| {
                let completed_id = entry.get("id").and_then(Value::as_str).unwrap_or("");
                (!completed_id.is_empty()
                    && ids.iter().any(|id| {
                        *id == completed_id || id.starts_with(&format!("{completed_id}:"))
                    }))
                    || (!name.is_empty()
                        && entry
                            .get("name")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value.eq_ignore_ascii_case(name)))
            })
        })
        .collect();
    serde_json::to_string(&visible).ok()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarItemInput {
    date_iso: String,
    #[serde(default)]
    meta_id: String,
    #[serde(default)]
    meta_type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: Option<String>,
    #[serde(default)]
    season_number: Option<i32>,
    #[serde(default)]
    episode_number: Option<i32>,
    #[serde(default)]
    episode_title: Option<String>,
    #[serde(default)]
    artwork_url: Option<String>,
    #[serde(default)]
    meta: Value,
    #[serde(default)]
    poster: Option<String>,
    #[serde(default)]
    episode_poster: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidatePlanRequest {
    #[serde(default)]
    groups: Vec<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseRowsRequest {
    meta: Value,
    #[serde(default)]
    detail: Value,
    #[serde(default)]
    videos: Vec<Value>,
    month_prefix: String,
    movie_label: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentPlanRequest {
    #[serde(default)]
    items: Vec<CalendarItemInput>,
    month_prefix: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeasonCandidatesRequest {
    #[serde(default)]
    seasons_count: Option<i32>,
    #[serde(default)]
    last_video_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetRowsRequest {
    #[serde(default)]
    items: Vec<Value>,
    #[serde(default = "default_max_rows")]
    max_rows: usize,
}

fn default_max_rows() -> usize {
    4
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotificationContentRequest {
    #[serde(default)]
    items: Vec<CalendarItemInput>,
    today_iso: String,
    #[serde(default)]
    already_notified_keys: Vec<String>,
    #[serde(default)]
    profile_id: Option<String>,
    notifications_enabled: Option<bool>,
    alert_new_episodes: Option<bool>,
    #[serde(default = "default_notification_key_limit")]
    max_stored_keys: usize,
}

fn default_notification_key_limit() -> usize {
    500
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseDetectionRequest {
    #[serde(default)]
    items: Vec<Value>,
    today_iso: String,
}

pub(crate) fn calendar_content_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ContentPlanRequest>(request_json).ok()?;
    let prefix = request.month_prefix.trim();
    if prefix.is_empty() {
        return serde_json::to_string(&json!([])).ok();
    }
    let mut seen = std::collections::HashSet::new();
    let mut filtered: Vec<&CalendarItemInput> = request
        .items
        .iter()
        .filter(|item| {
            let meta_id = if item.meta_id.trim().is_empty() {
                item.meta.get("id").and_then(Value::as_str).unwrap_or("")
            } else {
                &item.meta_id
            };
            item.date_iso.starts_with(prefix)
                && !meta_id.trim().is_empty()
                && seen.insert(format!(
                    "{}:{}:{}",
                    item.date_iso,
                    meta_id,
                    item.subtitle.as_deref().unwrap_or("")
                ))
        })
        .collect();
    filtered.sort_by(|a, b| {
        a.date_iso
            .cmp(&b.date_iso)
            .then_with(|| a.title.cmp(&b.title))
    });
    let out: Vec<Value> = filtered
        .iter()
        .map(|item| {
            let meta_id = if item.meta_id.trim().is_empty() {
                item.meta.get("id").and_then(Value::as_str).unwrap_or("")
            } else {
                &item.meta_id
            };
            let meta_type = if item.meta_type.trim().is_empty() {
                item.meta.get("type").and_then(Value::as_str).unwrap_or("")
            } else {
                &item.meta_type
            };
            json!({
                "dateIso": item.date_iso,
                "metaId": meta_id,
                "metaType": meta_type,
                "title": item.title,
                "subtitle": item.subtitle,
                "seasonNumber": item.season_number,
                "episodeNumber": item.episode_number,
                "episodeTitle": item.episode_title,
                "artworkUrl": item.artwork_url,
                "meta": item.meta,
                "poster": item.poster,
                "episodePoster": item.episode_poster
            })
        })
        .collect();
    serde_json::to_string(&out).ok()
}

pub(crate) fn desktop_calendar_read_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let prefix = request
        .get("monthPrefix")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut seen = std::collections::HashSet::new();
    let local_items: Vec<Value> = request
        .get("libraryItems")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("series"))
        .filter_map(|item| {
            let date_iso = item.get("nextEpisodeAirDate")?.as_str()?;
            if !prefix.is_empty() && !date_iso.starts_with(prefix) {
                return None;
            }
            let id = item.get("id")?.as_str()?;
            let key = format!("{}:{}", id, date_iso.get(..10).unwrap_or(date_iso));
            if !seen.insert(key.clone()) {
                return None;
            }
            Some(json!({
                "id": key,
                "title": item.get("name"),
                "name": item.get("name"),
                "dateIso": date_iso,
                "poster": item.get("nextEpisodePoster").or_else(|| item.get("poster")),
                "seriesPoster": item.get("poster"),
                "episodePoster": item.get("nextEpisodePoster"),
                "seasonNumber": item.get("nextEpisodeSeason"),
                "episodeNumber": item.get("nextEpisodeNumber"),
                "episodeTitle": item.get("nextEpisodeTitle"),
                "contentId": id,
                "seriesId": id,
                "metaType": item.get("type"),
            }))
        })
        .collect();
    let external_items: Vec<&Value> = request
        .get("externalItems")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| {
            prefix.is_empty()
                || item
                    .get("dateIso")
                    .and_then(Value::as_str)
                    .is_some_and(|date| date.starts_with(prefix))
        })
        .collect();
    serde_json::to_string(&json!({"items": request.get("plannedItems").and_then(Value::as_array).cloned().unwrap_or_default(), "localItems": local_items, "externalItems": external_items})).ok()
}

pub(crate) fn calendar_candidate_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<CandidatePlanRequest>(request_json).ok()?;
    let mut seen = std::collections::HashSet::new();
    let candidates: Vec<Value> = request
        .groups
        .into_iter()
        .flatten()
        .filter(|item| {
            let id = item.get("id").and_then(Value::as_str).unwrap_or("").trim();
            let content_type = item
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            !id.is_empty()
                && content_type != "catalog_folder"
                && seen.insert(format!("{}:{}", content_type.to_ascii_lowercase(), id))
        })
        .collect();
    serde_json::to_string(&candidates).ok()
}

pub(crate) fn calendar_release_rows_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ReleaseRowsRequest>(request_json).ok()?;
    let meta = &request.meta;
    let content_type = meta
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let meta_id = meta.get("id").and_then(Value::as_str).unwrap_or("");
    let title = meta.get("name").and_then(Value::as_str).unwrap_or(meta_id);
    if ["movie", "film"].contains(&content_type.as_str()) {
        let date_iso = meta
            .get("released")
            .and_then(Value::as_str)
            .and_then(|value| value.get(..10))?;
        if !date_iso.starts_with(&request.month_prefix) {
            return serde_json::to_string(&json!([])).ok();
        }
        let poster = usable_artwork(meta.get("poster").and_then(Value::as_str));
        return serde_json::to_string(&json!([{
            "dateIso": date_iso,
            "meta": meta,
            "title": title,
            "subtitle": request.movie_label,
            "poster": poster
        }]))
        .ok();
    }
    if !["series", "tv", "show", "anime"].contains(&content_type.as_str()) {
        return serde_json::to_string(&json!([])).ok();
    }
    let detail = &request.detail;
    let fallback_artwork = [
        meta.get("poster").and_then(Value::as_str),
        detail.get("poster").and_then(Value::as_str),
        meta.get("continueWatchingPoster").and_then(Value::as_str),
        meta.get("background").and_then(Value::as_str),
        meta.get("continueWatchingBackground")
            .and_then(Value::as_str),
        detail.get("background").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .find_map(|url| usable_artwork(Some(url)));
    let rows: Vec<Value> = request
        .videos
        .iter()
        .filter_map(|video| {
            let date_iso = video
                .get("released")
                .and_then(Value::as_str)
                .and_then(|value| value.get(..10))?;
            if !date_iso.starts_with(&request.month_prefix) {
                return None;
            }
            let season = video.get("season").and_then(Value::as_i64);
            let episode = video
                .get("number")
                .or_else(|| video.get("episode"))
                .and_then(Value::as_i64);
            let episode_code = match (season, episode) {
                (Some(s), Some(e)) => Some(format!("S{s}:E{e}")),
                (Some(s), None) => Some(format!("S{s}")),
                (None, Some(e)) => Some(format!("E{e}")),
                _ => None,
            };
            let episode_title = video
                .get("name")
                .or_else(|| video.get("title"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty());
            let subtitle = [episode_code.as_deref(), episode_title]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            Some(json!({
                "dateIso": date_iso,
                "meta": meta,
                "title": title,
                "subtitle": subtitle,
                "poster": fallback_artwork,
                "episodePoster": fallback_artwork,
                "seasonNumber": season,
                "episodeNumber": episode,
                "episodeTitle": episode_title
            }))
        })
        .collect();
    serde_json::to_string(&rows).ok()
}

fn usable_artwork(url: Option<&str>) -> Option<&str> {
    url.filter(|value| {
        let normalized = value.trim().to_ascii_lowercase();
        !normalized.is_empty()
            && normalized != "null"
            && !normalized.contains("default-poster")
            && !normalized.contains("placeholder")
    })
}

pub(crate) fn calendar_season_candidates_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<SeasonCandidatesRequest>(request_json).ok()?;
    let seasons_count = request.seasons_count.unwrap_or(1).max(1);
    let watched_season = request
        .last_video_id
        .as_deref()
        .and_then(|id| id.split(':').nth(1))
        .and_then(|s| s.parse::<i32>().ok());
    let focused: Vec<i32> = [
        watched_season,
        watched_season.map(|s| s + 1),
        Some(seasons_count),
    ]
    .into_iter()
    .flatten()
    .filter(|&s| s > 0 && s <= seasons_count)
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter()
    .collect();
    let full: Vec<i32> = if seasons_count <= 8 {
        (1..=seasons_count).collect()
    } else {
        focused.clone()
    };
    let mut result: Vec<i32> = focused
        .into_iter()
        .chain(full)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    result.truncate(12);
    serde_json::to_string(&result).ok()
}

pub(crate) fn calendar_widget_rows_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<WidgetRowsRequest>(request_json).ok()?;
    let rows: Vec<Value> = request
        .items
        .iter()
        .take(request.max_rows)
        .map(|item| {
            let date_iso = item.get("dateIso").and_then(Value::as_str).unwrap_or("");
            let title = item.get("title").and_then(Value::as_str).unwrap_or("");
            let subtitle = item
                .get("episodeTitle")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| item.get("subtitle").and_then(Value::as_str))
                .unwrap_or("");
            let season = item.get("seasonNumber").and_then(Value::as_i64);
            let episode = item.get("episodeNumber").and_then(Value::as_i64);
            let episode_text = match (season, episode) {
                (Some(s), Some(e)) => format!("S{}E{}", s, e),
                (Some(s), None) => format!("S{}", s),
                (None, Some(e)) => format!("E{}", e),
                _ => String::new(),
            };
            json!({
                "dateIso": date_iso,
                "title": title,
                "subtitle": subtitle,
                "episodeText": episode_text
            })
        })
        .collect();
    serde_json::to_string(&rows).ok()
}

pub(crate) fn calendar_notification_content_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<NotificationContentRequest>(request_json).ok()?;
    if request.notifications_enabled == Some(false) || request.alert_new_episodes == Some(false) {
        return serde_json::to_string(&json!({"items": [], "keys": []})).ok();
    }
    let profile_id = request.profile_id.as_deref().unwrap_or("");
    let mut items_out = Vec::new();
    let mut keys_out = Vec::new();
    for item in &request.items {
        if item.date_iso != request.today_iso || item.meta_type != "series" {
            continue;
        }
        let key = format!(
            "{}:{}:{}:{}",
            profile_id,
            item.date_iso,
            item.meta_id,
            item.subtitle.as_deref().unwrap_or("")
        );
        if request.already_notified_keys.contains(&key) {
            continue;
        }
        let title_key = if item.episode_number == Some(1) {
            "notification.new_season_released"
        } else {
            "notification.new_episode_released"
        };
        let body_text = match (item.season_number, item.episode_number) {
            (Some(s), Some(e)) => format!("{}:season:{}:episode:{}", item.title, s, e),
            _ => [Some(item.title.as_str()), item.subtitle.as_deref()]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" - "),
        };
        items_out.push(json!({
            "key": key,
            "titleKey": title_key,
            "bodyText": body_text,
            "metaId": item.meta_id,
            "dateIso": item.date_iso,
            "artworkUrl": item.artwork_url,
            "seasonNumber": item.season_number,
            "episodeNumber": item.episode_number,
            "title": item.title,
            "subtitle": item.subtitle,
            "episodeTitle": item.episode_title
        }));
        keys_out.push(key);
    }
    let mut stored_keys = request.already_notified_keys;
    stored_keys.extend(keys_out.iter().cloned());
    let start = stored_keys.len().saturating_sub(request.max_stored_keys);
    serde_json::to_string(
        &json!({"items": items_out, "keys": keys_out, "storedKeys": &stored_keys[start..]}),
    )
    .ok()
}

pub(crate) fn calendar_release_detection_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ReleaseDetectionRequest>(request_json).ok()?;
    let today = request.today_iso.trim();
    let released: Vec<&Value> = request
        .items
        .iter()
        .filter(|item| {
            item.get("dateIso")
                .and_then(Value::as_str)
                .is_some_and(|d| d == today)
        })
        .collect();
    serde_json::to_string(&released).ok()
}

pub(crate) fn calendar_items_from_meta_json(meta_json: &str, month_prefix: &str) -> Option<String> {
    let meta: Value = serde_json::from_str(meta_json).ok()?;
    let meta_id = meta.get("id").and_then(Value::as_str).unwrap_or("");
    let meta_name = meta.get("name").and_then(Value::as_str).unwrap_or("");
    let meta_poster = meta
        .get("poster")
        .and_then(Value::as_str)
        .or_else(|| meta.get("background").and_then(Value::as_str));
    let videos = meta.get("videos").and_then(Value::as_array)?;
    let mut items: Vec<Value> = Vec::new();
    for video in videos {
        let released = video.get("released").and_then(Value::as_str).unwrap_or("");
        let date_iso = match released.get(..10) {
            Some(d) => d,
            None => continue,
        };
        if !month_prefix.is_empty() && !date_iso.starts_with(month_prefix) {
            continue;
        }
        let season = video.get("season").and_then(Value::as_i64);
        let episode = video
            .get("episode")
            .or_else(|| video.get("number"))
            .and_then(Value::as_i64);
        let episode_code = match (season, episode) {
            (Some(s), Some(e)) => Some(format!("S{s}:E{e}")),
            _ => None,
        };
        let video_name = video
            .get("name")
            .or_else(|| video.get("title"))
            .and_then(Value::as_str);
        let subtitle = [episode_code.as_deref(), video_name]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let poster = video
            .get("thumbnail")
            .and_then(Value::as_str)
            .or(meta_poster);
        let video_id = video.get("id").and_then(Value::as_str).unwrap_or("");
        let key = format!("{meta_id}:{video_id}:{date_iso}");
        items.push(json!({
            "id": key,
            "title": meta_name,
            "name": video_name.unwrap_or(meta_name),
            "subtitle": subtitle,
            "dateIso": date_iso,
            "poster": poster,
        }));
    }
    serde_json::to_string(&items).ok()
}

/// Earliest video whose `released` date is strictly in the future, or None
/// if every video is already released, missing a date, or there are no videos.
/// Purely date-based (no current watch position needed) — unlike
/// `library_state::resolve_next_episode_json`, this works for items that
/// were never started.
pub(crate) fn next_unaired_episode_json(videos_json: &str, now_ms: i64) -> Option<String> {
    let videos: Vec<Value> = serde_json::from_str(videos_json).ok()?;
    let mut future: Vec<Value> = videos
        .into_iter()
        .filter(|v| v.get("released").and_then(Value::as_str).is_some())
        .filter(|v| !crate::library_state::is_episode_released(v, now_ms))
        .collect();
    future.sort_by(|a, b| {
        let ar = a.get("released").and_then(Value::as_str).unwrap_or("");
        let br = b.get("released").and_then(Value::as_str).unwrap_or("");
        ar.cmp(br)
    });
    let next = future.into_iter().next()?;
    serde_json::to_string(&next).ok()
}

fn end_of_current_week_ms(now_ms: i64) -> i64 {
    use chrono::{Datelike, Local, TimeZone};
    let now = Local
        .timestamp_millis_opt(now_ms)
        .single()
        .unwrap_or_else(chrono::Local::now);
    let days_until_sunday = (7 - now.weekday().num_days_from_sunday() as i64) % 7;
    let end_date = now.date_naive() + chrono::Duration::days(days_until_sunday);
    let end = end_date.and_hms_milli_opt(23, 59, 59, 999).unwrap();
    Local
        .from_local_datetime(&end)
        .single()
        .map(|d| d.timestamp_millis())
        .unwrap_or(now_ms)
}

fn parse_date_ms(raw: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|d| d.timestamp_millis())
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis())
        })
        .ok()
}

pub(crate) fn partition_this_week_json(
    items_json: &str,
    now_ms: i64,
    keep_scheduled: bool,
) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let week_end = end_of_current_week_ms(now_ms);

    let mut this_week: Vec<Value> = Vec::new();
    let mut this_week_ids = std::collections::HashSet::new();
    for item in &items {
        if item.get("continueWatchingBadge").and_then(Value::as_str) != Some("scheduledEpisode") {
            continue;
        }
        let Some(released_at) = item.get("newEpisodeReleasedAt").and_then(Value::as_str) else {
            continue;
        };
        let Some(released_ms) = parse_date_ms(released_at) else {
            continue;
        };
        if released_ms <= week_end {
            this_week.push(item.clone());
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                this_week_ids.insert(id.to_string());
            }
        }
    }

    let continue_watching: Vec<Value> = if keep_scheduled {
        items
    } else {
        items
            .into_iter()
            .filter(|m| {
                let id = m.get("id").and_then(Value::as_str).unwrap_or("");
                !this_week_ids.contains(id)
            })
            .collect()
    };

    serde_json::to_string(&json!({ "thisWeek": this_week, "continueWatching": continue_watching }))
        .ok()
}

pub(crate) fn calendar_item_matches_month_json(item_json: &str, month_prefix: &str) -> bool {
    if month_prefix.is_empty() {
        return true;
    }
    serde_json::from_str::<Value>(item_json)
        .ok()
        .and_then(|v| {
            v.get("dateIso")
                .and_then(Value::as_str)
                .map(|d| d.starts_with(month_prefix))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn candidate_plan_merges_groups_and_deduplicates_content() {
        let result: Value = serde_json::from_str(
            &calendar_candidate_plan_json(
                r#"{"groups":[[{"id":"tt1","type":"series","name":"Library"}],[{"id":"tt1","type":"series","name":"Progress"},{"id":"tt2","type":"anime","name":"Provider"}]]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
        assert_eq!(result[0]["name"], json!("Library"));
        assert_eq!(result[1]["id"], json!("tt2"));
    }

    #[test]
    fn release_rows_build_series_episodes_and_movie_releases() {
        let series: Value = serde_json::from_str(
            &calendar_release_rows_json(
                r#"{"meta":{"id":"tt1","type":"tv","name":"Show","poster":"poster.jpg"},"videos":[{"released":"2026-07-20T00:00:00Z","season":2,"number":1,"name":"Premiere"}],"monthPrefix":"2026-07","movieLabel":"Movie"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(series[0]["subtitle"], json!("S2:E1 Premiere"));
        assert_eq!(series[0]["meta"]["id"], json!("tt1"));

        let movie: Value = serde_json::from_str(
            &calendar_release_rows_json(
                r#"{"meta":{"id":"tt2","type":"film","name":"Film","released":"2026-07-21"},"videos":[],"monthPrefix":"2026-07","movieLabel":"Movie"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(movie[0]["subtitle"], json!("Movie"));
    }

    #[test]
    fn content_plan_filters_deduplicates_and_sorts_by_date_then_title() {
        let result: Value = serde_json::from_str(
            &calendar_content_plan_json(
                r#"{"monthPrefix":"2026-06","items":[
                    {"dateIso":"2026-06-15","metaId":"tt1","metaType":"series","title":"B","subtitle":"E2"},
                    {"dateIso":"2026-06-10","metaId":"tt2","metaType":"movie","title":"A"},
                    {"dateIso":"2026-06-15","metaId":"tt1","metaType":"series","title":"B","subtitle":"E2"},
                    {"dateIso":"2026-05-01","metaId":"tt3","metaType":"movie","title":"Old"}
                ]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["metaId"], "tt2");
        assert_eq!(arr[1]["metaId"], "tt1");
    }

    #[test]
    fn season_candidates_covers_watched_next_and_last_season() {
        let result: Value = serde_json::from_str(
            &calendar_season_candidates_json(r#"{"seasonsCount":5,"lastVideoId":"tt1:2:3"}"#)
                .unwrap(),
        )
        .unwrap();
        let seasons: Vec<i64> = result
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        assert!(seasons.contains(&2));
        assert!(seasons.contains(&3));
        assert!(seasons.contains(&5));
    }

    #[test]
    fn widget_rows_truncates_to_max_rows() {
        let items = (0..6)
            .map(|i| {
                json!({
                    "dateIso": format!("2026-06-{:02}", i + 1),
                    "title": format!("Show {}", i),
                    "subtitle": "",
                    "seasonNumber": 1,
                    "episodeNumber": i + 1
                })
            })
            .collect::<Vec<_>>();
        let request = json!({"items": items, "maxRows": 4});
        let result: Value =
            serde_json::from_str(&calendar_widget_rows_json(&request.to_string()).unwrap())
                .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 4);
    }

    #[test]
    fn notification_content_skips_already_notified_and_non_today_items() {
        let request = json!({
            "items": [
                {"dateIso":"2026-06-10","metaId":"tt1","metaType":"series","title":"Show","subtitle":"E1","seasonNumber":1,"episodeNumber":1},
                {"dateIso":"2026-06-11","metaId":"tt2","metaType":"series","title":"Show2","subtitle":"E1","seasonNumber":1,"episodeNumber":1}
            ],
            "todayIso": "2026-06-10",
            "alreadyNotifiedKeys": [":2026-06-10:tt1:E1"],
            "notificationsEnabled": true,
            "alertNewEpisodes": true
        });
        let result: Value = serde_json::from_str(
            &calendar_notification_content_json(&request.to_string()).unwrap(),
        )
        .unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn notification_content_returns_render_fields_and_bounded_stored_keys() {
        let request = json!({
            "items": [{"dateIso":"2026-06-10","metaId":"tt2","metaType":"series","title":"Show","subtitle":"Episode","episodeTitle":"Pilot"}],
            "todayIso": "2026-06-10",
            "alreadyNotifiedKeys": ["old-1", "old-2"],
            "maxStoredKeys": 2
        });
        let result: Value = serde_json::from_str(
            &calendar_notification_content_json(&request.to_string()).unwrap(),
        )
        .unwrap();
        assert_eq!(result["items"][0]["episodeTitle"], "Pilot");
        assert_eq!(result["storedKeys"].as_array().unwrap().len(), 2);
        assert_eq!(result["storedKeys"][0], "old-2");
    }

    #[test]
    fn next_unaired_episode_picks_earliest_future_date() {
        let now_ms = chrono::DateTime::parse_from_rfc3339("2026-06-16T00:00:00Z")
            .unwrap()
            .timestamp_millis();
        let videos = json!([
            {"id": "v1", "released": "2026-06-01T00:00:00Z"},
            {"id": "v2", "released": "2026-07-10T00:00:00Z"},
            {"id": "v3", "released": "2026-06-20T00:00:00Z"},
            {"id": "v4"}
        ]);
        let result: Value =
            serde_json::from_str(&next_unaired_episode_json(&videos.to_string(), now_ms).unwrap())
                .unwrap();
        assert_eq!(result["id"], "v3");
    }

    #[test]
    fn next_unaired_episode_returns_none_when_nothing_upcoming() {
        let now_ms = chrono::DateTime::parse_from_rfc3339("2026-06-16T00:00:00Z")
            .unwrap()
            .timestamp_millis();
        let videos = json!([
            {"id": "v1", "released": "2026-06-01T00:00:00Z"},
            {"id": "v2"}
        ]);
        assert!(next_unaired_episode_json(&videos.to_string(), now_ms).is_none());
    }

    #[test]
    fn release_detection_returns_only_today_items() {
        let request = json!({
            "todayIso": "2026-06-10",
            "items": [
                {"dateIso":"2026-06-10","metaId":"tt1"},
                {"dateIso":"2026-06-11","metaId":"tt2"},
                {"dateIso":"2026-06-10","metaId":"tt3"}
            ]
        });
        let result: Value =
            serde_json::from_str(&calendar_release_detection_json(&request.to_string()).unwrap())
                .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }
}
