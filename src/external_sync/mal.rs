use serde_json::{Value, json};

pub(crate) fn mal_list_update_json(args_json: &str, watched: bool) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let meta = args.get("meta")?;
    if meta.get("type").and_then(Value::as_str) != Some("series") {
        return None;
    }
    let mal_id = meta
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| id.strip_prefix("mal:"))
        .filter(|id| !id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit()))
        .and_then(|id| id.parse::<i64>().ok())?;
    if !watched {
        return serde_json::to_string(&json!({
            "malId": mal_id,
            "watchedEpisodes": Value::Null,
            "status": "plan_to_watch",
        }))
        .ok();
    }
    let highest_episode = args
        .get("episodes")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|episode| episode.get("number").and_then(Value::as_i64))
        .max()?;
    let completed = meta
        .get("episodesCount")
        .and_then(Value::as_i64)
        .is_some_and(|count| highest_episode >= count);
    serde_json::to_string(&json!({
        "malId": mal_id,
        "watchedEpisodes": highest_episode,
        "status": if completed { "completed" } else { "watching" },
    }))
    .ok()
}
