use crate::content_identity::{base_content_id, parse_episode_locator, parse_video_id_json};
use serde_json::{json, Map, Value};

const TRAKT_API_BASE_URL: &str = "https://api.trakt.tv";

pub(crate) fn trakt_has_client(api_key: &str) -> bool {
    !api_key.trim().is_empty()
}

pub(crate) fn trakt_bearer(token: &str) -> String {
    format!("Bearer {token}")
}

pub(crate) fn trakt_scrobble_url(action: &str) -> String {
    format!("{TRAKT_API_BASE_URL}/scrobble/{action}")
}

pub(crate) fn trakt_playback_url(content_type: Option<&str>) -> String {
    match content_type.filter(|value| !value.trim().is_empty()) {
        Some(content_type) => format!("{TRAKT_API_BASE_URL}/sync/playback/{content_type}"),
        None => format!("{TRAKT_API_BASE_URL}/sync/playback"),
    }
}

pub(crate) fn trakt_token_expires_at(created_at_seconds: i64, expires_in_seconds: i64) -> i64 {
    let refresh_buffer_seconds = 5 * 60;
    let effective_expires_in = (expires_in_seconds - refresh_buffer_seconds).max(0);
    (created_at_seconds * 1000) + (effective_expires_in * 1000)
}

fn number_to_i32(value: &Value) -> Option<i32> {
    value.as_i64().and_then(|value| i32::try_from(value).ok())
}

pub(crate) fn trakt_content_id_from_ids_json(ids_json: &str) -> Option<String> {
    let ids: Value = serde_json::from_str(ids_json).ok()?;
    ids.get("imdb")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            ids.get("tmdb")
                .and_then(number_to_i32)
                .map(|id| format!("tmdb:{id}"))
        })
        .or_else(|| {
            ids.get("tvdb")
                .and_then(number_to_i32)
                .map(|id| format!("tvdb:{id}"))
        })
        .or_else(|| {
            ids.get("slug")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(|slug| format!("trakt:{slug}"))
        })
        .or_else(|| {
            ids.get("trakt")
                .and_then(number_to_i32)
                .map(|id| format!("trakt:{id}"))
        })
}

pub(crate) fn trakt_ids_from_content_id_json(raw_id: &str) -> Option<String> {
    let imdb = regex::Regex::new(r"tt\d+")
        .ok()?
        .find(raw_id)
        .map(|m| m.as_str().to_string());
    let mut ids = Map::new();
    if let Some(imdb) = imdb {
        ids.insert("imdb".to_string(), Value::String(imdb));
        return serde_json::to_string(&Value::Object(ids)).ok();
    }

    let prefix_number = |prefix: &str| {
        raw_id
            .strip_prefix(prefix)
            .and_then(|rest| rest.split(':').next())
            .and_then(|value| value.parse::<i32>().ok())
    };

    if let Some(tmdb) = prefix_number("tmdb:") {
        ids.insert("tmdb".to_string(), json!(tmdb));
    } else if let Some(tvdb) = prefix_number("tvdb:") {
        ids.insert("tvdb".to_string(), json!(tvdb));
    } else if let Some(trakt) = prefix_number("trakt:") {
        ids.insert("trakt".to_string(), json!(trakt));
    } else if let Some(tmdb) = raw_id
        .split(':')
        .next()
        .and_then(|value| value.parse::<i32>().ok())
    {
        ids.insert("tmdb".to_string(), json!(tmdb));
    }

    if ids.is_empty() {
        None
    } else {
        serde_json::to_string(&Value::Object(ids)).ok()
    }
}

pub(crate) fn trakt_episode_locator_json(video_id: &str) -> Option<String> {
    let (_, season, episode) = parse_episode_locator(video_id)?;
    serde_json::to_string(&json!({
        "season": season,
        "episode": episode
    }))
    .ok()
}

pub(crate) fn trakt_show_id_from_episode_id(video_id: &str) -> String {
    if parse_episode_locator(video_id).is_some() {
        base_content_id(video_id)
    } else {
        video_id.to_string()
    }
}

pub(crate) fn trakt_scrobble_media_id(
    parent_id: &str,
    video_id: Option<&str>,
    media_type: &str,
) -> String {
    if media_type != "series" {
        return video_id.unwrap_or(parent_id).to_string();
    }
    let Some(video_id) = video_id.filter(|value| !value.is_empty()) else {
        return parent_id.to_string();
    };
    let Some((_, season, episode)) = parse_episode_locator(video_id) else {
        return video_id.to_string();
    };
    format!("{parent_id}:{season}:{episode}")
}

pub(crate) fn trakt_oauth_error_code(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    value
        .get("error")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn episode_season_number(episode: &Value) -> Option<(i32, i32)> {
    let parsed = episode
        .get("id")
        .and_then(Value::as_str)
        .and_then(parse_episode_locator);
    let season = episode
        .get("season")
        .and_then(number_to_i32)
        .or_else(|| parsed.as_ref().map(|(_, season, _)| *season));
    let number = episode
        .get("number")
        .and_then(number_to_i32)
        .or_else(|| parsed.as_ref().map(|(_, _, episode)| *episode));
    season.zip(number)
}

pub(crate) fn trakt_history_request_json(meta_json: &str, episodes_json: &str) -> Option<String> {
    let meta: Value = serde_json::from_str(meta_json).ok()?;
    let episodes: Vec<Value> = serde_json::from_str(episodes_json).unwrap_or_default();
    let meta_id = meta.get("id").and_then(Value::as_str).unwrap_or("");
    let ids_json = trakt_ids_from_content_id_json(meta_id).or_else(|| {
        episodes
            .first()
            .and_then(|episode| episode.get("id").and_then(Value::as_str))
            .and_then(trakt_ids_from_content_id_json)
    })?;
    let ids: Value = serde_json::from_str(&ids_json).ok()?;

    if meta.get("type").and_then(Value::as_str) == Some("movie") {
        return serde_json::to_string(&json!({
            "movies": [{ "ids": ids }]
        }))
        .ok();
    }

    let target_episodes = if episodes.is_empty() {
        meta.get("lastVideoId")
            .and_then(Value::as_str)
            .or_else(|| meta.get("id").and_then(Value::as_str))
            .and_then(parse_episode_locator)
            .map(|(_, season, episode)| {
                vec![json!({
                    "season": season,
                    "number": episode
                })]
            })
            .unwrap_or_default()
    } else {
        episodes
    };

    let mut seasons = std::collections::BTreeMap::<i32, Vec<i32>>::new();
    for episode in target_episodes.iter().filter_map(episode_season_number) {
        seasons.entry(episode.0).or_default().push(episode.1);
    }
    if seasons.is_empty() {
        return None;
    }

    let seasons = seasons
        .into_iter()
        .map(|(season, mut episodes)| {
            episodes.sort_unstable();
            episodes.dedup();
            json!({
                "number": season,
                "episodes": episodes.into_iter().map(|number| json!({ "number": number })).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&json!({
        "shows": [{
            "ids": ids,
            "seasons": seasons
        }]
    }))
    .ok()
}

fn trakt_id_from_source(source: &Value) -> Option<String> {
    let ids = source.get("ids")?;
    ids.get("imdb")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            ids.get("tmdb")
                .and_then(Value::as_i64)
                .map(|n| format!("tmdb:{n}"))
        })
}

pub(crate) fn trakt_playback_items_to_library_json(items_json: &str) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let result: Vec<Value> = items
        .iter()
        .filter_map(trakt_playback_item_to_library)
        .collect();
    serde_json::to_string(&result).ok()
}

fn trakt_playback_item_to_library(item: &Value) -> Option<Value> {
    let movie = item.get("movie");
    let show = item.get("show");
    let episode = item.get("episode");
    let source = movie.or(show)?;
    let id = trakt_id_from_source(source)?;
    let progress = item.get("progress").and_then(Value::as_f64).unwrap_or(0.0);
    if progress < 1.0 {
        return None;
    }
    let title = source
        .get("title")
        .or_else(|| source.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Untitled");
    let episode_title = episode
        .and_then(|e| e.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let ep_runtime = episode
        .and_then(|e| e.get("runtime"))
        .and_then(Value::as_f64);
    let runtime_min = ep_runtime
        .or_else(|| source.get("runtime").and_then(Value::as_f64))
        .unwrap_or(if movie.is_some() { 100.0 } else { 45.0 });
    let duration_sec = (runtime_min * 60.0) as i64;
    let time_offset_sec = ((progress / 100.0) * duration_sec as f64).round() as i64;
    let content_type = if movie.is_some() { "movie" } else { "series" };
    let last_video_id = if let Some(ep) = episode {
        let show_imdb = show
            .and_then(|s| s.get("ids"))
            .and_then(|ids| ids.get("imdb"))
            .and_then(Value::as_str)
            .unwrap_or("trakt");
        let season = ep.get("season").and_then(Value::as_i64).unwrap_or(0);
        let number = ep.get("number").and_then(Value::as_i64).unwrap_or(0);
        format!("{show_imdb}:{season}:{number}")
    } else {
        id.clone()
    };
    let episode_season = episode
        .and_then(|e| e.get("season"))
        .and_then(Value::as_i64);
    let episode_number = episode
        .and_then(|e| e.get("number"))
        .and_then(Value::as_i64);
    let saved_at = item.get("paused_at").and_then(Value::as_str).unwrap_or("");
    Some(json!({
        "id": id,
        "name": title,
        "type": content_type,
        "timeOffset": time_offset_sec,
        "duration": duration_sec,
        "lastVideoId": last_video_id,
        "lastEpisodeName": if episode_title.is_empty() { Value::Null } else { Value::String(episode_title.to_string()) },
        "lastEpisodeSeason": episode_season,
        "lastEpisodeNumber": episode_number,
        "savedAt": saved_at,
        "reason": "trakt"
    }))
}

pub(crate) fn trakt_watchlist_to_items_json(movies_json: &str, shows_json: &str) -> Option<String> {
    let movies: Vec<Value> = serde_json::from_str(movies_json).unwrap_or_default();
    let shows: Vec<Value> = serde_json::from_str(shows_json).unwrap_or_default();
    let mut items: Vec<Value> = Vec::new();
    for entry in &movies {
        let movie = entry.get("movie")?;
        let id = trakt_id_from_source(movie)?;
        let name = movie.get("title").and_then(Value::as_str).unwrap_or("");
        items.push(json!({ "id": id, "name": name, "type": "movie", "source": "trakt" }));
    }
    for entry in &shows {
        let show = entry.get("show")?;
        let id = trakt_id_from_source(show)?;
        let name = show.get("title").and_then(Value::as_str).unwrap_or("");
        items.push(json!({ "id": id, "name": name, "type": "series", "source": "trakt" }));
    }
    serde_json::to_string(&items).ok()
}

pub(crate) fn trakt_watched_to_ids_json(movies_json: &str, shows_json: &str) -> Option<String> {
    let movies: Vec<Value> = serde_json::from_str(movies_json).unwrap_or_default();
    let shows: Vec<Value> = serde_json::from_str(shows_json).unwrap_or_default();
    let mut ids: serde_json::Map<String, Value> = serde_json::Map::new();
    for entry in &movies {
        if let Some(imdb) = entry
            .get("movie")
            .and_then(|m| m.get("ids"))
            .and_then(|ids| ids.get("imdb"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            ids.insert(imdb.to_string(), Value::Bool(true));
        }
    }
    for entry in &shows {
        let imdb = match entry
            .get("show")
            .and_then(|s| s.get("ids"))
            .and_then(|ids| ids.get("imdb"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            Some(s) => s,
            None => continue,
        };
        let seasons = entry
            .get("seasons")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for season in &seasons {
            let s_num = season.get("number").and_then(Value::as_i64).unwrap_or(0);
            let episodes = season
                .get("episodes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for ep in &episodes {
                let e_num = ep.get("number").and_then(Value::as_i64).unwrap_or(0);
                if s_num > 0 && e_num > 0 {
                    ids.insert(format!("{imdb}:{s_num}:{e_num}"), Value::Bool(true));
                }
            }
        }
    }
    serde_json::to_string(&Value::Object(ids)).ok()
}

pub(crate) fn stremio_watchlist_to_items_json(items_json: &str) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let out: Vec<Value> = items
        .iter()
        .filter(|item| {
            item.get("removed").and_then(Value::as_bool) != Some(true)
                && item.get("temp").and_then(Value::as_bool) != Some(true)
        })
        .filter_map(|item| {
            let id = item
                .get("_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())?;
            if crate::content_identity::parse_episode_locator(id).is_some() {
                return None;
            }
            let name = item.get("name").and_then(Value::as_str).unwrap_or("");
            let kind = match item.get("type").and_then(Value::as_str) {
                Some("movie") => "movie",
                _ => "series",
            };
            let mut entry = json!({ "id": id, "name": name, "type": kind, "source": "stremio" });
            if let Some(poster) = item
                .get("poster")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                entry["poster"] = json!(poster);
            }
            Some(entry)
        })
        .collect();
    serde_json::to_string(&out).ok()
}

pub(crate) fn stremio_watched_to_ids_json(items_json: &str) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let mut ids: serde_json::Map<String, Value> = serde_json::Map::new();
    for item in &items {
        let flagged = item.get("state").is_some_and(|s| {
            s.get("flaggedWatched").and_then(Value::as_i64).unwrap_or(0) == 1
                || s.get("timesWatched").and_then(Value::as_i64).unwrap_or(0) > 0
        });
        if !flagged {
            continue;
        }
        if let Some(id) = item
            .get("_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            ids.insert(id.to_string(), Value::Bool(true));
        }
    }
    serde_json::to_string(&Value::Object(ids)).ok()
}

pub(crate) fn merge_external_watchlist_json(local_json: &str, external_json: &str) -> String {
    let mut local: Vec<Value> = serde_json::from_str(local_json).unwrap_or_default();
    let external: Vec<Value> = serde_json::from_str(external_json).unwrap_or_default();
    let local_ids: std::collections::HashSet<String> = local
        .iter()
        .filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    for item in external {
        if let Some(id) = item.get("id").and_then(Value::as_str) {
            if !local_ids.contains(id) {
                local.push(item);
            }
        }
    }
    serde_json::to_string(&local).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) fn merge_external_watched_json(local_json: &str, external_json: &str) -> String {
    let mut local: serde_json::Map<String, Value> =
        serde_json::from_str(local_json).unwrap_or_default();
    let external: serde_json::Map<String, Value> =
        serde_json::from_str(external_json).unwrap_or_default();
    for (id, val) in external {
        if val.as_bool() == Some(true) && !local.contains_key(&id) {
            local.insert(id, Value::Bool(true));
        }
    }
    serde_json::to_string(&Value::Object(local)).unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn merge_continue_watching_lists_json(
    local_json: &str,
    external_json: &str,
    progress_json: &str,
) -> Option<String> {
    let local: Vec<Value> = serde_json::from_str(local_json).unwrap_or_default();
    let external: Vec<Value> = serde_json::from_str(external_json).unwrap_or_default();
    let progress: serde_json::Map<String, Value> =
        serde_json::from_str(progress_json).unwrap_or_default();

    fn item_id(item: &Value) -> String {
        item.get("id")
            .or_else(|| item.get("_id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    fn saved_at_ms(item: &Value) -> i64 {
        item.get("savedAt")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt: chrono::DateTime<chrono::FixedOffset>| dt.timestamp_millis())
            .unwrap_or(0)
    }

    let local_by_id: std::collections::HashMap<String, &Value> =
        local.iter().map(|item| (item_id(item), item)).collect();
    let external_by_id: std::collections::HashMap<String, &Value> =
        external.iter().map(|item| (item_id(item), item)).collect();

    fn local_saved_at_from_progress(progress: &serde_json::Map<String, Value>, id: &str) -> i64 {
        progress
            .get(id)
            .and_then(|entry| entry.get("savedAt"))
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt: chrono::DateTime<chrono::FixedOffset>| dt.timestamp_millis())
            .unwrap_or(0)
    }

    let mut merged: Vec<Value> = Vec::new();
    for ext_item in &external {
        let id = item_id(ext_item);
        let local_time = local_saved_at_from_progress(&progress, &id);
        let ext_time = saved_at_ms(ext_item);
        if local_time > ext_time {
            let local_item = local_by_id.get(&id).copied();
            merged.push(local_item.cloned().unwrap_or_else(|| ext_item.clone()));
        } else {
            merged.push(ext_item.clone());
        }
    }
    for local_item in &local {
        let id = item_id(local_item);
        if !external_by_id.contains_key(&id) {
            merged.push(local_item.clone());
        }
    }

    serde_json::to_string(&merged).ok()
}

pub(crate) fn simkl_watching_to_items_json(shows_json: &str, movies_json: &str) -> Option<String> {
    let shows: Vec<Value> = serde_json::from_str(shows_json).unwrap_or_default();
    let movies: Vec<Value> = serde_json::from_str(movies_json).unwrap_or_default();
    let mut items: Vec<Value> = Vec::new();
    for entry in &shows {
        let show = entry.get("show")?;
        let ids = show.get("ids")?;
        let imdb = ids
            .get("imdb")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())?;
        let title = show.get("title").and_then(Value::as_str).unwrap_or("");
        let poster = show
            .get("poster")
            .and_then(Value::as_str)
            .map(|p| format!("https://simkl.in/posters/{p}_m.jpg"));
        let saved_at = entry
            .get("last_watched")
            .and_then(Value::as_str)
            .unwrap_or_default();
        items.push(json!({
            "id": imdb, "type": "series", "name": title,
            "poster": poster, "continueWatchingBadge": "upNext",
            "savedAt": saved_at, "reason": "simkl"
        }));
    }
    for entry in &movies {
        let movie = entry.get("movie")?;
        let ids = movie.get("ids")?;
        let imdb = ids
            .get("imdb")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())?;
        let title = movie.get("title").and_then(Value::as_str).unwrap_or("");
        let poster = movie
            .get("poster")
            .and_then(Value::as_str)
            .map(|p| format!("https://simkl.in/posters/{p}_m.jpg"));
        let saved_at = entry
            .get("last_watched")
            .and_then(Value::as_str)
            .unwrap_or_default();
        items.push(json!({
            "id": imdb, "type": "movie", "name": title,
            "poster": poster, "savedAt": saved_at, "reason": "simkl"
        }));
    }
    serde_json::to_string(&items).ok()
}

pub(crate) fn simkl_watchlist_to_items_json(shows_json: &str, movies_json: &str) -> Option<String> {
    let shows: Vec<Value> = serde_json::from_str(shows_json).unwrap_or_default();
    let movies: Vec<Value> = serde_json::from_str(movies_json).unwrap_or_default();
    let mut items: Vec<Value> = Vec::new();
    for entry in &shows {
        let show = entry.get("show")?;
        let ids = show.get("ids")?;
        let imdb = ids
            .get("imdb")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())?;
        let title = show.get("title").and_then(Value::as_str).unwrap_or("");
        let poster = show
            .get("poster")
            .and_then(Value::as_str)
            .map(|p| format!("https://simkl.in/posters/{p}_m.jpg"));
        items.push(json!({ "id": imdb, "name": title, "type": "series", "source": "simkl", "poster": poster }));
    }
    for entry in &movies {
        let movie = entry.get("movie")?;
        let ids = movie.get("ids")?;
        let imdb = ids
            .get("imdb")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())?;
        let title = movie.get("title").and_then(Value::as_str).unwrap_or("");
        let poster = movie
            .get("poster")
            .and_then(Value::as_str)
            .map(|p| format!("https://simkl.in/posters/{p}_m.jpg"));
        items.push(json!({ "id": imdb, "name": title, "type": "movie", "source": "simkl", "poster": poster }));
    }
    serde_json::to_string(&items).ok()
}

pub(crate) fn simkl_watched_to_ids_json(shows_json: &str, movies_json: &str) -> Option<String> {
    let shows: Vec<Value> = serde_json::from_str(shows_json).unwrap_or_default();
    let movies: Vec<Value> = serde_json::from_str(movies_json).unwrap_or_default();
    let mut ids: serde_json::Map<String, Value> = serde_json::Map::new();
    for entry in &shows {
        if let Some(imdb) = entry
            .get("show")
            .and_then(|s| s.get("ids"))
            .and_then(|i| i.get("imdb"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            ids.insert(imdb.to_string(), Value::Bool(true));
        }
    }
    for entry in &movies {
        if let Some(imdb) = entry
            .get("movie")
            .and_then(|m| m.get("ids"))
            .and_then(|i| i.get("imdb"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            ids.insert(imdb.to_string(), Value::Bool(true));
        }
    }
    serde_json::to_string(&Value::Object(ids)).ok()
}

/// Replaces or merges external continue-watching items from one provider.
/// Items from other providers are kept; items from `provider` are replaced.
/// Deduplicates by `id`, keeping the entry with the most recent `savedAt`.
pub(crate) fn replace_external_continue_watching_json(
    existing_json: &str,
    provider: Option<&str>,
    items_json: &str,
) -> String {
    let existing: Vec<Value> = serde_json::from_str(existing_json).unwrap_or_default();
    let incoming: Vec<Value> = serde_json::from_str(items_json).unwrap_or_default();

    let incoming_filtered: Vec<Value> = incoming
        .into_iter()
        .filter(|item| {
            let id = item.get("id").and_then(Value::as_str).unwrap_or("").trim();
            let offset = item
                .get("timeOffset")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let duration = item.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
            !id.is_empty() && offset > 0.0 && duration > 0.0
        })
        .collect();

    let base: Vec<Value> = if let Some(prov) = provider {
        existing
            .into_iter()
            .filter(|item| item.get("reason").and_then(Value::as_str) != Some(prov))
            .collect()
    } else {
        Vec::new()
    };

    let combined = base.into_iter().chain(incoming_filtered);
    let mut by_id: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for item in combined {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let item_time = item
            .get("savedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match by_id.get(&id) {
            Some(prev) => {
                let prev_time = prev.get("savedAt").and_then(Value::as_str).unwrap_or("");
                if item_time.as_str() > prev_time {
                    by_id.insert(id, item);
                }
            }
            None => {
                by_id.insert(id, item);
            }
        }
    }

    let result: Vec<Value> = by_id.into_values().collect();
    serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) fn trakt_playback_items_dedup_json(items_json: &str) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;

    fn saved_at_str(item: &Value) -> &str {
        item.get("savedAt").and_then(Value::as_str).unwrap_or("")
    }

    let mut best: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let cur = saved_at_str(&item).to_string();
        match best.get(&id) {
            None => {
                best.insert(id, item);
            }
            Some(existing) if cur.as_str() > saved_at_str(existing) => {
                best.insert(id, item);
            }
            _ => {}
        }
    }

    let mut deduped: Vec<Value> = best.into_values().collect();
    deduped.sort_by(|a, b| saved_at_str(b).cmp(saved_at_str(a)));
    serde_json::to_string(&deduped).ok()
}

pub(crate) fn trakt_mark_watched_body_json(video_ids_json: &str) -> Option<String> {
    let video_ids: Vec<String> = serde_json::from_str(video_ids_json).ok()?;
    let mut movie_ids: Vec<Value> = Vec::new();
    let mut shows: std::collections::HashMap<
        String,
        (Value, std::collections::BTreeMap<i64, Vec<i64>>),
    > = std::collections::HashMap::new();

    for vid in &video_ids {
        let parsed_json = parse_video_id_json(vid);
        let parsed: Value = match serde_json::from_str(&parsed_json) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ids_json = match trakt_ids_from_content_id_json(vid) {
            Some(j) => j,
            None => continue,
        };
        let ids: Value = match serde_json::from_str(&ids_json) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if parsed
            .get("isEpisode")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let season = parsed.get("season").and_then(Value::as_i64).unwrap_or(1);
            let episode = parsed.get("episode").and_then(Value::as_i64).unwrap_or(1);
            let show_id = parsed
                .get("imdb")
                .or_else(|| parsed.get("tmdb"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if show_id.is_empty() {
                continue;
            }
            let entry = shows
                .entry(show_id)
                .or_insert_with(|| (ids, std::collections::BTreeMap::new()));
            entry.1.entry(season).or_default().push(episode);
        } else {
            movie_ids.push(json!({ "ids": ids }));
        }
    }

    let show_entries: Vec<Value> = shows
        .into_values()
        .map(|(ids, seasons)| {
            let seasons_arr: Vec<Value> = seasons
                .into_iter()
                .map(|(season, mut episodes)| {
                    episodes.sort_unstable();
                    episodes.dedup();
                    json!({
                        "number": season,
                        "episodes": episodes.into_iter().map(|n| json!({ "number": n })).collect::<Vec<_>>()
                    })
                })
                .collect();
            json!({ "ids": ids, "seasons": seasons_arr })
        })
        .collect();

    let mut body = serde_json::Map::new();
    if !movie_ids.is_empty() {
        body.insert("movies".into(), movie_ids.into());
    }
    if !show_entries.is_empty() {
        body.insert("shows".into(), show_entries.into());
    }
    if body.is_empty() {
        return None;
    }
    serde_json::to_string(&Value::Object(body)).ok()
}

pub(crate) fn simkl_match_episode_json(episodes_json: &str, target_json: &str) -> Option<String> {
    let episodes: Vec<Value> = serde_json::from_str(episodes_json).ok()?;
    let target: Value = serde_json::from_str(target_json).ok()?;
    let release_date = target
        .get("releaseDate")
        .and_then(Value::as_str)
        .unwrap_or("");
    let title = target
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let title = title.trim();

    let matched = if !release_date.is_empty() {
        episodes.iter().find(|ep| {
            ep.get("date")
                .and_then(Value::as_str)
                .is_some_and(|d| d.starts_with(release_date))
        })
    } else {
        None
    };

    let matched = matched.or_else(|| {
        if title.is_empty() {
            return None;
        }
        episodes.iter().find(|ep| {
            ep.get("title")
                .and_then(Value::as_str)
                .is_some_and(|t| t.to_lowercase().trim() == title)
        })
    })?;

    let season = matched.get("season").and_then(Value::as_i64)?;
    let episode = matched.get("episode").and_then(Value::as_i64)?;
    serde_json::to_string(&json!({ "season": season, "episode": episode })).ok()
}

#[derive(serde::Deserialize)]
struct AnilistEntry {
    status: Option<String>,
    progress: Option<f64>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<i64>,
    media: Option<AnilistMedia>,
}

#[derive(serde::Deserialize)]
struct AnilistMedia {
    id: Option<i64>,
    title: Option<AnilistTitle>,
    #[serde(rename = "coverImage")]
    cover_image: Option<AnilistCover>,
    #[serde(rename = "bannerImage")]
    banner_image: Option<String>,
    episodes: Option<i64>,
    #[serde(rename = "seasonYear")]
    season_year: Option<i64>,
    genres: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
struct AnilistTitle {
    english: Option<String>,
    romaji: Option<String>,
    native: Option<String>,
}

#[derive(serde::Deserialize)]
struct AnilistCover {
    #[serde(rename = "extraLarge")]
    extra_large: Option<String>,
    large: Option<String>,
}

fn anilist_media_to_item(media: &AnilistMedia, media_id: i64) -> Map<String, Value> {
    let title = media
        .title
        .as_ref()
        .and_then(|t| {
            [&t.english, &t.romaji, &t.native]
                .into_iter()
                .find_map(|s| s.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("AniList {media_id}"));

    let mut item = Map::new();
    item.insert("id".to_string(), json!(format!("anilist:{media_id}")));
    item.insert("name".to_string(), json!(title));
    item.insert("type".to_string(), json!("series"));
    let poster = media
        .cover_image
        .as_ref()
        .and_then(|c| c.extra_large.as_deref().or(c.large.as_deref()));
    if let Some(poster) = poster {
        item.insert("poster".to_string(), json!(poster));
    }
    if let Some(banner) = &media.banner_image {
        item.insert("background".to_string(), json!(banner));
    }
    if let Some(year) = media.season_year {
        item.insert("year".to_string(), json!(year));
    }
    if let Some(genres) = &media.genres {
        item.insert("genres".to_string(), json!(genres));
    }
    item.insert("anilistId".to_string(), json!(media_id));
    if let Some(episodes) = media.episodes {
        item.insert("totalEpisodes".to_string(), json!(episodes));
    }
    item
}

fn anilist_updated_at(updated_at_seconds: Option<i64>, now_ms: i64) -> String {
    let seconds = updated_at_seconds.unwrap_or(0);
    let ms = if seconds > 0 { seconds * 1000 } else { now_ms };
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}

fn mark_watched_through(watched: &mut serde_json::Map<String, Value>, id: &str, through: i64) {
    for ep in 1..=through.max(0) {
        watched.insert(format!("{id}:1:{ep}"), Value::Bool(true));
    }
}

pub(crate) fn anilist_entries_to_sync(entries: &[Value], now_ms: i64) -> Value {
    let mut watchlist: Vec<Value> = Vec::new();
    let mut completed: Vec<Value> = Vec::new();
    let mut dropped: Vec<Value> = Vec::new();
    let mut watching: Vec<Value> = Vec::new();
    let mut watched = Map::new();
    let mut progress = Map::new();

    for raw in entries {
        let Ok(entry) = serde_json::from_value::<AnilistEntry>(raw.clone()) else {
            continue;
        };
        let Some(media) = &entry.media else {
            continue;
        };
        let Some(media_id) = media.id else {
            continue;
        };
        let item = anilist_media_to_item(media, media_id);
        let id = format!("anilist:{media_id}");
        let status = entry.status.as_deref().unwrap_or("").to_uppercase();
        let progress_episode = entry
            .progress
            .map(|p| p.floor().max(0.0) as i64)
            .unwrap_or(0);
        let updated_at = anilist_updated_at(entry.updated_at, now_ms);

        match status.as_str() {
            "PLANNING" => {
                let mut it = item.clone();
                it.insert("inWatchlist".to_string(), Value::Bool(true));
                watchlist.push(Value::Object(it));
            }
            "COMPLETED" => {
                let mut it = item.clone();
                it.insert("statusChangedAt".to_string(), json!(updated_at));
                completed.push(Value::Object(it));
                let through = if progress_episode > 0 {
                    progress_episode
                } else {
                    media.episodes.unwrap_or(0)
                };
                mark_watched_through(&mut watched, &id, through);
            }
            "DROPPED" | "PAUSED" => {
                let mut it = item.clone();
                it.insert("statusChangedAt".to_string(), json!(updated_at));
                dropped.push(Value::Object(it));
                mark_watched_through(&mut watched, &id, progress_episode);
            }
            "CURRENT" | "REPEATING" if progress_episode > 0 => {
                let last_video_id = format!("{id}:1:{progress_episode}");
                let episode_name = format!("Episode {progress_episode}");
                let mut it = item.clone();
                it.insert("lastVideoId".to_string(), json!(last_video_id));
                it.insert("lastEpisodeSeason".to_string(), json!(1));
                it.insert("lastEpisodeNumber".to_string(), json!(progress_episode));
                it.insert("lastEpisodeName".to_string(), json!(episode_name));
                it.insert("timeOffset".to_string(), json!(1));
                it.insert("duration".to_string(), json!(1));
                watching.push(Value::Object(it));
                progress.insert(
                    id.clone(),
                    json!({
                        "meta": Value::Object(item.clone()),
                        "lastVideoId": last_video_id,
                        "lastEpisodeSeason": 1,
                        "lastEpisodeNumber": progress_episode,
                        "lastEpisodeName": episode_name,
                        "timeOffset": 1,
                        "duration": 1,
                        "savedAt": updated_at,
                    }),
                );
                mark_watched_through(&mut watched, &id, progress_episode);
            }
            _ => {}
        }
    }

    json!({
        "watchlist": watchlist,
        "completed": completed,
        "dropped": dropped,
        "watching": watching,
        "watched": Value::Object(watched),
        "progress": Value::Object(progress),
    })
}

pub(crate) fn merge_library_items_by_id(local: &[Value], incoming: &[Value]) -> Value {
    fn item_id(item: &Value) -> String {
        match item.get("id") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    }

    let mut order: Vec<String> = Vec::new();
    let mut by_id: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for item in local {
        let id = item_id(item);
        if !by_id.contains_key(&id) {
            order.push(id.clone());
        }
        by_id.insert(id, item.clone());
    }
    for item in incoming {
        let id = item_id(item);
        let merged = match (by_id.get(&id), item) {
            (Some(Value::Object(existing)), Value::Object(inc)) => {
                let mut m = existing.clone();
                for (k, v) in inc {
                    m.insert(k.clone(), v.clone());
                }
                Value::Object(m)
            }
            _ => item.clone(),
        };
        if !by_id.contains_key(&id) {
            order.push(id.clone());
        }
        by_id.insert(id, merged);
    }

    let merged: Vec<Value> = order.iter().filter_map(|id| by_id.remove(id)).collect();
    Value::Array(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn anilist_current_entries_build_progress_and_watched_keys() {
        let entries = r#"[
            {"status":"CURRENT","progress":3,"updatedAt":1700000000,"media":{"id":5,"title":{"romaji":"Show"},"episodes":12}},
            {"status":"PLANNING","media":{"id":6,"title":{"english":"Other"}}}
        ]"#;
        let plan = anilist_entries_to_sync(
            serde_json::from_str::<Vec<Value>>(entries).unwrap().as_slice(),
            0,
        );
        assert_eq!(plan["watching"][0]["lastVideoId"], "anilist:5:1:3");
        assert_eq!(plan["watched"]["anilist:5:1:2"], Value::Bool(true));
        assert_eq!(
            plan["progress"]["anilist:5"]["savedAt"],
            "2023-11-14T22:13:20.000Z"
        );
        assert_eq!(plan["watchlist"][0]["inWatchlist"], Value::Bool(true));
        assert_eq!(plan["watchlist"][0]["name"], "Other");
    }

    #[test]
    fn merge_by_id_overlays_incoming_fields_onto_local_items() {
        let local: Vec<Value> =
            serde_json::from_str(r#"[{"id":"a","name":"Old","poster":"p"},{"id":"b","name":"Keep"}]"#)
                .unwrap();
        let incoming: Vec<Value> =
            serde_json::from_str(r#"[{"id":"a","name":"New"},{"id":"c","name":"Added"}]"#).unwrap();
        let merged = merge_library_items_by_id(&local, &incoming);
        assert_eq!(merged[0]["name"], "New");
        assert_eq!(merged[0]["poster"], "p");
        assert_eq!(merged[1]["name"], "Keep");
        assert_eq!(merged[2]["name"], "Added");
    }

    #[test]
    fn stremio_episode_entries_become_watched_keys_not_watchlist_items() {
        let items = r#"[
            {"_id":"tt1","name":"A Movie","type":"movie","poster":"p.jpg","state":{"flaggedWatched":1}},
            {"_id":"tt2","name":"A Show","type":"series","state":{"flaggedWatched":0}},
            {"_id":"tt2:1:3","name":"A Show","type":"series","state":{"flaggedWatched":1}},
            {"_id":"tt3","name":"Removed","type":"movie","removed":true,"state":null}
        ]"#;
        let watchlist: Vec<Value> =
            serde_json::from_str(&stremio_watchlist_to_items_json(items).unwrap()).unwrap();
        let ids: Vec<&str> = watchlist
            .iter()
            .filter_map(|i| i.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["tt1", "tt2"]);

        let watched: Value =
            serde_json::from_str(&stremio_watched_to_ids_json(items).unwrap()).unwrap();
        assert_eq!(watched.get("tt1"), Some(&Value::Bool(true)));
        assert_eq!(watched.get("tt2:1:3"), Some(&Value::Bool(true)));
        assert_eq!(watched.get("tt2"), None);
    }

    #[test]
    fn trakt_ids_support_stremio_episode_ids() {
        assert_eq!(
            trakt_ids_from_content_id_json("tt1234567:1:2")
                .and_then(|json| serde_json::from_str::<Value>(&json).ok())
                .and_then(|ids| ids.get("imdb").and_then(Value::as_str).map(str::to_owned))
                .as_deref(),
            Some("tt1234567")
        );
        assert_eq!(
            trakt_ids_from_content_id_json("tmdb:42:1:2")
                .and_then(|json| serde_json::from_str::<Value>(&json).ok())
                .and_then(|ids| ids.get("tmdb").and_then(Value::as_i64)),
            Some(42)
        );
    }

    #[test]
    fn history_request_builds_show_seasons_from_episode_ids() {
        let request = trakt_history_request_json(
            r#"{"id":"tt1234567","name":"Show","type":"series","poster":null}"#,
            r#"[{"id":"tt1234567:1:2","name":null,"season":null,"number":null,"released":null,"thumbnail":null}]"#,
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("history request");

        assert_eq!(
            request
                .get("shows")
                .and_then(Value::as_array)
                .and_then(|shows| shows.first())
                .and_then(|show| show.get("seasons"))
                .and_then(Value::as_array)
                .and_then(|seasons| seasons.first())
                .and_then(|season| season.get("number"))
                .and_then(Value::as_i64),
            Some(1)
        );
        assert!(request.get("movies").is_none());
    }

    #[test]
    fn trakt_oauth_error_code_extracts_structured_error() {
        assert_eq!(
            trakt_oauth_error_code(r#"{"error":"authorization_pending"}"#).as_deref(),
            Some("authorization_pending")
        );
        assert_eq!(trakt_oauth_error_code("{}"), None);
    }

    #[test]
    fn trakt_mark_watched_body_groups_episodes_by_show_and_dedupes() {
        let body = trakt_mark_watched_body_json(
            &json!([
                "tt1234567:1:1",
                "tt1234567:1:2",
                "tt1234567:1:1",
                "tt7654321"
            ])
            .to_string(),
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("body");

        let movies = body["movies"].as_array().unwrap();
        assert_eq!(movies.len(), 1);
        assert_eq!(movies[0]["ids"]["imdb"], "tt7654321");

        let shows = body["shows"].as_array().unwrap();
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0]["ids"]["imdb"], "tt1234567");
        let seasons = shows[0]["seasons"].as_array().unwrap();
        assert_eq!(seasons.len(), 1);
        assert_eq!(seasons[0]["number"], 1);
        // The duplicate tt1234567:1:1 must not produce a duplicate episode entry.
        let episodes = seasons[0]["episodes"].as_array().unwrap();
        assert_eq!(episodes.len(), 2);
        assert_eq!(episodes[0]["number"], 1);
        assert_eq!(episodes[1]["number"], 2);
    }

    #[test]
    fn trakt_mark_watched_body_is_none_for_unrecognized_ids() {
        assert_eq!(
            trakt_mark_watched_body_json(&json!(["not-an-id"]).to_string()),
            None
        );
    }
}
