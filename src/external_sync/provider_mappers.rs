use super::{ranked_winner, saved_at_ms, trakt_id_from_source, trakt_ids_from_content_id_json};
use crate::content_identity::parse_video_id_json;
use serde_json::{json, Value};

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

/// Replaces or merges external continue-watching items from one provider.
/// Items from other providers are kept; items from `provider` are replaced.
/// Deduplicates by `id`, keeping the entry with the most recent `savedAt`.
pub(crate) fn replace_external_continue_watching_json(
    existing_json: &str,
    provider: Option<&str>,
    items_json: &str,
    source_of_truth: Option<&str>,
    ranking_mode: Option<&str>,
    continue_watching_days: Option<i64>,
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
            let within_window =
                continue_watching_days
                    .filter(|days| *days > 0)
                    .is_none_or(|days| {
                        saved_at_ms(item)
                            >= chrono::Utc::now().timestamp_millis() - days * 86_400_000
                    });
            !id.is_empty() && offset > 0.0 && duration > 0.0 && within_window
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
        match by_id.get(&id) {
            Some(prev) => {
                let item_reason = item.get("reason").and_then(Value::as_str);
                let prev_reason = prev.get("reason").and_then(Value::as_str);
                let item_wins = if source_of_truth.is_some() && source_of_truth == item_reason {
                    true
                } else if source_of_truth.is_some() && source_of_truth == prev_reason {
                    false
                } else {
                    ranked_winner(
                        &item,
                        saved_at_ms(&item),
                        prev,
                        saved_at_ms(prev),
                        ranking_mode,
                    )
                };
                if item_wins {
                    by_id.insert(id, item);
                }
            }
            None => {
                by_id.insert(id, item);
            }
        }
    }

    let mut result: Vec<Value> = by_id.into_values().collect();
    result.sort_by(|a, b| saved_at_ms(b).cmp(&saved_at_ms(a)));
    serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) fn trakt_playback_items_dedup_json(items_json: &str) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;

    fn saved_at_str(item: &Value) -> &str {
        item.get("savedAt").and_then(Value::as_str).unwrap_or("")
    }

    fn episode_rank(item: &Value) -> Option<(i64, i64)> {
        Some((
            item.get("lastEpisodeSeason").and_then(Value::as_i64)?,
            item.get("lastEpisodeNumber").and_then(Value::as_i64)?,
        ))
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
            Some(existing) => {
                let incoming_rank = episode_rank(&item);
                let existing_rank = episode_rank(existing);
                let incoming_is_watched =
                    item.get("continueWatchingBadge").and_then(Value::as_str) == Some("upNext");
                let existing_is_watched = existing
                    .get("continueWatchingBadge")
                    .and_then(Value::as_str)
                    == Some("upNext");
                let incoming_wins = match (incoming_rank, existing_rank) {
                    (Some(incoming_rank), Some(existing_rank))
                        if incoming_is_watched || existing_is_watched =>
                    {
                        incoming_rank > existing_rank
                    }
                    _ => cur.as_str() > saved_at_str(existing),
                };
                if incoming_wins {
                    best.insert(id, item);
                }
            }
        }
    }

    let mut deduped: Vec<Value> = best.into_values().collect();
    deduped.sort_by(|a, b| saved_at_str(b).cmp(saved_at_str(a)));
    serde_json::to_string(&deduped).ok()
}

pub(crate) fn trakt_mark_watched_body_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let video_ids: Vec<String> = request
        .as_array()
        .cloned()
        .and_then(|value| serde_json::from_value(Value::Array(value)).ok())
        .or_else(|| {
            request
                .get("videoIds")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
        })?;
    let watched_at = request
        .get("watchedAtMs")
        .and_then(Value::as_i64)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|value| value.to_rfc3339());
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
            movie_ids.push(json!({ "ids": ids, "watched_at": watched_at }));
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
                        "episodes": episodes.into_iter().map(|n| json!({ "number": n, "watched_at": watched_at })).collect::<Vec<_>>()
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

pub(crate) fn simkl_mark_watched_body_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let video_ids = args.get("videoIds")?.as_array()?;
    let meta_type = args
        .pointer("/meta/type")
        .and_then(Value::as_str)
        .unwrap_or("movie");
    let watched_at = args
        .get("watchedAtMs")
        .and_then(Value::as_i64)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|value| value.to_rfc3339());
    let mut movies = Vec::new();
    let mut shows: std::collections::HashMap<
        String,
        (Value, std::collections::BTreeMap<i64, Vec<i64>>),
    > = std::collections::HashMap::new();
    for video_id in video_ids.iter().filter_map(Value::as_str) {
        let parsed: Value = serde_json::from_str(&parse_video_id_json(video_id)).ok()?;
        let ids = parsed
            .get("imdb")
            .and_then(Value::as_str)
            .map(|id| json!({"imdb": id}))
            .or_else(|| {
                parsed
                    .get("tmdb")
                    .and_then(Value::as_str)
                    .and_then(|id| id.parse::<i64>().ok())
                    .map(|id| json!({"tmdb": id}))
            });
        let Some(ids) = ids else { continue };
        if parsed.get("isEpisode").and_then(Value::as_bool) == Some(true) {
            let season = parsed.get("season").and_then(Value::as_i64).unwrap_or(1);
            let episode = parsed.get("episode").and_then(Value::as_i64).unwrap_or(1);
            let key = ids.to_string();
            let entry = shows
                .entry(key)
                .or_insert_with(|| (ids, std::collections::BTreeMap::new()));
            entry.1.entry(season).or_default().push(episode);
        } else if meta_type == "series" {
            shows
                .entry(ids.to_string())
                .or_insert_with(|| (ids, std::collections::BTreeMap::new()));
        } else {
            movies.push(json!({"ids": ids, "watched_at": watched_at.clone().unwrap_or_else(|| "now".to_string())}));
        }
    }
    let show_values = shows
        .into_values()
        .map(|(ids, seasons)| {
            if seasons.is_empty() {
                return json!({"ids": ids});
            }
            json!({"ids": ids, "seasons": seasons.into_iter().map(|(number, mut episodes)| {
            episodes.sort_unstable(); episodes.dedup();
            let episodes = episodes.into_iter().map(|number| {
                let mut episode = json!({"number": number});
                if let Some(timestamp) = watched_at.as_ref() {
                    episode["watched_at"] = Value::String(timestamp.clone());
                }
                episode
            }).collect::<Vec<_>>();
            json!({"number": number, "episodes": episodes})
        }).collect::<Vec<_>>()})
        })
        .collect::<Vec<_>>();
    if movies.is_empty() && show_values.is_empty() {
        return None;
    }
    serde_json::to_string(&json!({"movies": movies, "shows": show_values})).ok()
}

pub(crate) fn simkl_watchlist_body_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let id = args.get("id")?.as_str()?;
    let parsed: Value = serde_json::from_str(&parse_video_id_json(id)).ok()?;
    let ids = parsed
        .get("imdb")
        .and_then(Value::as_str)
        .map(|id| json!({"imdb": id}))
        .or_else(|| {
            parsed
                .get("tmdb")
                .and_then(Value::as_str)
                .and_then(|id| id.parse::<i64>().ok())
                .map(|id| json!({"tmdb": id}))
        })?;
    let entry = if args.get("command").and_then(Value::as_str) == Some("remove") {
        json!({"ids": ids})
    } else {
        json!({"ids": ids, "to": "plantowatch"})
    };
    let body = if args.get("contentType").and_then(Value::as_str) == Some("series") {
        json!({"shows": [entry]})
    } else {
        json!({"movies": [entry]})
    };
    serde_json::to_string(&body).ok()
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

pub(crate) fn trakt_related_lookup_slug(lookup_json: &str, want_type: &str) -> Option<String> {
    let lookup: Vec<Value> = serde_json::from_str(lookup_json).ok()?;
    lookup
        .first()?
        .get(want_type)?
        .get("ids")?
        .get("slug")?
        .as_str()
        .map(|s| s.to_string())
}

pub(crate) fn trakt_related_items_to_metas_json(
    related_json: &str,
    content_type: &str,
) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(related_json).ok()?;
    let metas: Vec<Value> = items
        .iter()
        .filter_map(|item| {
            let ids = item.get("ids")?;
            let id = ids
                .get("imdb")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .or_else(|| {
                    ids.get("tmdb")
                        .and_then(Value::as_i64)
                        .map(|t| format!("tmdb:{t}"))
                })?;
            let name = item.get("title").and_then(Value::as_str)?;
            let mut meta = json!({ "id": id, "type": content_type, "name": name });
            if let Some(year) = item.get("year").and_then(Value::as_i64) {
                meta["releaseInfo"] = json!(year.to_string());
            }
            Some(meta)
        })
        .collect();
    if metas.is_empty() {
        return None;
    }
    serde_json::to_string(&metas).ok()
}

pub(crate) fn simkl_lookup_id_for_type(lookup_json: &str, want_type: &str) -> Option<i64> {
    let lookup: Vec<Value> = serde_json::from_str(lookup_json).ok()?;
    lookup
        .iter()
        .find(|item| item.get("type").and_then(Value::as_str) == Some(want_type))
        .and_then(|item| item.get("ids")?.get("simkl")?.as_i64())
}

pub(crate) fn simkl_recommendation_candidates_json(detail_json: &str) -> Option<String> {
    let detail: Value = serde_json::from_str(detail_json).ok()?;
    let recs = detail.get("users_recommendations")?.as_array()?;
    let candidates: Vec<Value> = recs.iter().take(15).cloned().collect();
    serde_json::to_string(&candidates).ok()
}

pub(crate) fn simkl_poster_url(path: &str) -> String {
    format!("https://wsrv.nl/?url=https://simkl.in/posters/{path}_c.webp&q=90")
}

pub(crate) fn simkl_recommendation_to_meta_json(
    rec_json: &str,
    resolved_imdb: &str,
) -> Option<String> {
    let rec: Value = serde_json::from_str(rec_json).ok()?;
    let title = rec.get("title").and_then(Value::as_str)?;
    let type_str = rec.get("type").and_then(Value::as_str).unwrap_or("movie");
    let meta_type = if type_str == "tv" { "series" } else { "movie" };
    let mut meta = json!({ "id": resolved_imdb, "type": meta_type, "name": title });
    if let Some(poster) = rec.get("poster").and_then(Value::as_str) {
        meta["poster"] = json!(simkl_poster_url(poster));
    }
    if let Some(year) = rec.get("year").and_then(Value::as_i64) {
        meta["releaseInfo"] = json!(year.to_string());
    }
    serde_json::to_string(&meta).ok()
}
