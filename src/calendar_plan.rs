use serde::Deserialize;
use serde_json::{json, Value};

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
            item.date_iso.starts_with(prefix)
                && !item.meta_id.trim().is_empty()
                && seen.insert(format!(
                    "{}:{}:{}",
                    item.date_iso,
                    item.meta_id,
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
            json!({
                "dateIso": item.date_iso,
                "metaId": item.meta_id,
                "metaType": item.meta_type,
                "title": item.title,
                "subtitle": item.subtitle,
                "seasonNumber": item.season_number,
                "episodeNumber": item.episode_number,
                "episodeTitle": item.episode_title,
                "artworkUrl": item.artwork_url
            })
        })
        .collect();
    serde_json::to_string(&out).ok()
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
        .chain(full.into_iter())
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
            _ => {
                [Some(item.title.as_str()), item.subtitle.as_deref()]
                    .into_iter()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" - ")
            }
        };
        items_out.push(json!({
            "key": key,
            "titleKey": title_key,
            "bodyText": body_text,
            "metaId": item.meta_id,
            "dateIso": item.date_iso,
            "artworkUrl": item.artwork_url,
            "seasonNumber": item.season_number,
            "episodeNumber": item.episode_number
        }));
        keys_out.push(key);
    }
    serde_json::to_string(&json!({"items": items_out, "keys": keys_out})).ok()
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
    let meta_poster = meta.get("poster").and_then(Value::as_str)
        .or_else(|| meta.get("background").and_then(Value::as_str));
    let videos = meta.get("videos").and_then(Value::as_array)?;
    let mut items: Vec<Value> = Vec::new();
    for video in videos {
        let released = video.get("released").and_then(Value::as_str).unwrap_or("");
        let date_iso = match released.get(..10) { Some(d) => d, None => continue };
        if !month_prefix.is_empty() && !date_iso.starts_with(month_prefix) { continue; }
        let season = video.get("season").and_then(Value::as_i64);
        let episode = video.get("episode").or_else(|| video.get("number")).and_then(Value::as_i64);
        let episode_code = match (season, episode) {
            (Some(s), Some(e)) => Some(format!("S{s}:E{e}")),
            _ => None,
        };
        let video_name = video.get("name").or_else(|| video.get("title")).and_then(Value::as_str);
        let subtitle = [episode_code.as_deref(), video_name].into_iter().flatten().collect::<Vec<_>>().join(" ");
        let poster = video.get("thumbnail").and_then(Value::as_str).or(meta_poster);
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

pub(crate) fn calendar_item_matches_month_json(item_json: &str, month_prefix: &str) -> bool {
    if month_prefix.is_empty() { return true; }
    serde_json::from_str::<Value>(item_json).ok()
        .and_then(|v| v.get("dateIso").and_then(Value::as_str).map(|d| d.starts_with(month_prefix)))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

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
            &calendar_season_candidates_json(
                r#"{"seasonsCount":5,"lastVideoId":"tt1:2:3"}"#,
            )
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
        let result: Value =
            serde_json::from_str(&calendar_notification_content_json(&request.to_string()).unwrap())
                .unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 0);
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
