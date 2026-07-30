use crate::external_sync::trakt_id_from_source;
use serde_json::{Value, json};

fn simkl_entries(json: &str, key: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(json).ok() {
        Some(Value::Array(entries)) => entries,
        Some(Value::Object(response)) => response
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn simkl_last_watched_episode(entry: &Value) -> Option<(i64, i64)> {
    let from_code = entry
        .get("last_watched")
        .and_then(Value::as_str)
        .and_then(|value| value.strip_prefix('S'))
        .and_then(|value| value.split_once('E'))
        .and_then(|(season, episode)| {
            Some((season.parse::<i64>().ok()?, episode.parse::<i64>().ok()?))
        })
        .filter(|(season, episode)| *season > 0 && *episode > 0);
    if from_code.is_some() {
        return from_code;
    }

    entry
        .get("seasons")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|season| {
            let season_number = season.get("number").and_then(Value::as_i64).unwrap_or(0);
            season
                .get("episodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |episode| {
                    let episode_number = episode.get("number").and_then(Value::as_i64)?;
                    (season_number > 0 && episode_number > 0)
                        .then_some((season_number, episode_number))
                })
        })
        .max()
}

pub(crate) fn simkl_watching_to_items_json(shows_json: &str, movies_json: &str) -> Option<String> {
    let shows = simkl_entries(shows_json, "shows");
    let movies = simkl_entries(movies_json, "movies");
    let mut items: Vec<Value> = Vec::new();
    for entry in &shows {
        let Some(show) = entry.get("show") else {
            continue;
        };
        let Some(id) = trakt_id_from_source(show) else {
            continue;
        };
        let title = show.get("title").and_then(Value::as_str).unwrap_or("");
        let poster = show
            .get("poster")
            .and_then(Value::as_str)
            .map(|p| format!("https://simkl.in/posters/{p}_m.jpg"));
        let saved_at = entry
            .get("last_watched_at")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let last_episode = simkl_last_watched_episode(entry);
        let last_video_id =
            last_episode.map(|(season, episode)| format!("{id}:{season}:{episode}"));
        items.push(json!({
            "id": id, "type": "series", "name": title,
            "poster": poster, "continueWatchingBadge": "upNext",
            "timeOffset": 1, "duration": 1,
            "lastVideoId": last_video_id,
            "lastEpisodeSeason": last_episode.map(|(season, _)| season),
            "lastEpisodeNumber": last_episode.map(|(_, episode)| episode),
            "savedAt": saved_at, "reason": "simkl"
        }));
    }
    for entry in &movies {
        let Some(movie) = entry.get("movie") else {
            continue;
        };
        let Some(id) = trakt_id_from_source(movie) else {
            continue;
        };
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
            "id": id, "type": "movie", "name": title,
            "poster": poster, "timeOffset": 1, "duration": 1,
            "savedAt": saved_at, "reason": "simkl"
        }));
    }
    serde_json::to_string(&items).ok()
}

pub(crate) fn trakt_watched_shows_to_items_json(shows_json: &str) -> Option<String> {
    let shows: Vec<Value> = serde_json::from_str(shows_json).unwrap_or_default();
    let mut items = Vec::new();

    for entry in shows {
        let Some(show) = entry.get("show") else {
            continue;
        };
        let Some(id) = trakt_id_from_source(show) else {
            continue;
        };
        let aired_episodes = show.get("aired_episodes").and_then(Value::as_i64);
        let completed = entry.get("completed").and_then(Value::as_i64);
        if aired_episodes.is_some_and(|aired| completed.unwrap_or(0) >= aired) {
            continue;
        }

        let mut last_episode: Option<(i64, i64, Option<String>)> = None;
        for season in entry
            .get("seasons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let season_number = season.get("number").and_then(Value::as_i64).unwrap_or(0);
            if season_number <= 0 {
                continue;
            }
            for episode in season
                .get("episodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let number = episode.get("number").and_then(Value::as_i64).unwrap_or(0);
                if number <= 0 {
                    continue;
                }
                let watched_at = episode
                    .get("last_watched_at")
                    .or_else(|| episode.get("watched_at"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let rank = (season_number, number);
                let replace =
                    last_episode
                        .as_ref()
                        .is_none_or(|(previous_season, previous_number, _)| {
                            rank > (*previous_season, *previous_number)
                        });
                if replace {
                    last_episode = Some((season_number, number, watched_at));
                }
            }
        }
        let Some((season, number, watched_at)) = last_episode else {
            continue;
        };
        let saved_at = entry
            .get("last_watched_at")
            .and_then(Value::as_str)
            .or(watched_at.as_deref())
            .unwrap_or("");
        let title = show.get("title").and_then(Value::as_str).unwrap_or("");
        items.push(json!({
            "id": id,
            "type": "series",
            "name": title,
            "continueWatchingBadge": "upNext",
            "lastVideoId": format!("{id}:{season}:{number}"),
            "lastEpisodeSeason": season,
            "lastEpisodeNumber": number,
            "timeOffset": 1,
            "duration": 1,
            "savedAt": saved_at,
            "reason": "trakt"
        }));
    }

    serde_json::to_string(&items).ok()
}

pub(crate) fn simkl_watchlist_to_items_json(shows_json: &str, movies_json: &str) -> Option<String> {
    let shows = simkl_entries(shows_json, "shows");
    let movies = simkl_entries(movies_json, "movies");
    let mut items: Vec<Value> = Vec::new();
    for entry in &shows {
        let Some(show) = entry.get("show") else {
            continue;
        };
        let Some(id) = trakt_id_from_source(show) else {
            continue;
        };
        let title = show.get("title").and_then(Value::as_str).unwrap_or("");
        let poster = show
            .get("poster")
            .and_then(Value::as_str)
            .map(|p| format!("https://simkl.in/posters/{p}_m.jpg"));
        items.push(json!({ "id": id, "name": title, "type": "series", "source": "simkl", "poster": poster }));
    }
    for entry in &movies {
        let Some(movie) = entry.get("movie") else {
            continue;
        };
        let Some(id) = trakt_id_from_source(movie) else {
            continue;
        };
        let title = movie.get("title").and_then(Value::as_str).unwrap_or("");
        let poster = movie
            .get("poster")
            .and_then(Value::as_str)
            .map(|p| format!("https://simkl.in/posters/{p}_m.jpg"));
        items.push(json!({ "id": id, "name": title, "type": "movie", "source": "simkl", "poster": poster }));
    }
    serde_json::to_string(&items).ok()
}

pub(crate) fn simkl_watched_to_ids_json(shows_json: &str, movies_json: &str) -> Option<String> {
    let shows = simkl_entries(shows_json, "shows");
    let movies = simkl_entries(movies_json, "movies");
    let mut ids: serde_json::Map<String, Value> = serde_json::Map::new();
    for entry in &shows {
        if let Some(id) = entry.get("show").and_then(trakt_id_from_source) {
            ids.insert(id, Value::Bool(true));
        }
    }
    for entry in &movies {
        if let Some(id) = entry.get("movie").and_then(trakt_id_from_source) {
            ids.insert(id, Value::Bool(true));
        }
    }
    serde_json::to_string(&Value::Object(ids)).ok()
}
