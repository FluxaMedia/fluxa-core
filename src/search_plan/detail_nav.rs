use serde_json::{Value, json};

pub(crate) fn detail_series_lookup_id(raw_id: &str) -> String {
    let trimmed = raw_id.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(imdb) = extract_imdb_id(trimmed) {
        return imdb;
    }
    // Strip trailing season:episode parts (e.g. "kitsu:777:1:2" -> "kitsu:777", "base:1:2" -> "base")
    let parts: Vec<&str> = trimmed.split(':').collect();
    if parts.len() >= 3 {
        let last = parts[parts.len() - 1];
        let second_last = parts[parts.len() - 2];
        if last.parse::<i32>().is_ok() && second_last.parse::<i32>().is_ok() {
            return parts[..parts.len() - 2].join(":");
        }
    }
    trimmed.to_string()
}

fn extract_imdb_id(raw: &str) -> Option<String> {
    let mut start = 0;
    let bytes = raw.as_bytes();
    while start < bytes.len() {
        if bytes[start] == b't' && start + 2 < bytes.len() && bytes[start + 1] == b't' {
            let end = bytes[start..]
                .iter()
                .take_while(|&&b| b.is_ascii_digit() || (b == b't' && start == 0))
                .count();
            let candidate = &raw[start..start + end];
            if candidate.starts_with("tt")
                && candidate[2..].chars().all(|c| c.is_ascii_digit())
                && candidate.len() > 3
            {
                return Some(candidate.to_string());
            }
        }
        start += 1;
    }
    None
}

pub(crate) fn detail_season_load_plan_json(request_json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(request_json).ok()?;
    let saved_video_id = value
        .get("savedVideoId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let seasons_count = value
        .get("seasonsCount")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1) as i32;

    let saved_season = saved_video_id
        .split(':')
        .nth(1)
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);

    let first_season = if saved_season > 0 && saved_season <= seasons_count {
        saved_season
    } else {
        1
    };

    serde_json::to_string(&json!({
        "firstSeasonToLoad": first_season,
        "savedSeason": if saved_season > 0 { json!(saved_season) } else { Value::Null }
    }))
    .ok()
}
