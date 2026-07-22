use crate::content_identity::{base_content_id, imdb_regex, parse_episode_locator};
use serde_json::{json, Map, Value};

pub(crate) fn external_sync_response_action(provider: &str, status_code: i64) -> &'static str {
    if (200..300).contains(&status_code) {
        "stamp_success"
    } else if status_code == 401 && provider == "mal" {
        "refresh_credentials"
    } else if status_code == 401 {
        "clear_credentials"
    } else {
        "keep_credentials"
    }
}

pub(crate) fn external_sync_refresh_retry_action(status_code: Option<i64>) -> &'static str {
    match status_code {
        Some(code) if (200..300).contains(&code) => "stamp_success",
        Some(401) => "clear_credentials",
        _ => "keep_credentials",
    }
}

pub(crate) fn simkl_history_request_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let imdb_id = args.get("imdbId")?.as_str()?.trim();
    if imdb_id.is_empty() {
        return None;
    }
    let is_series = args.get("isSeries")?.as_bool()?;
    let ids = json!({ "imdb": imdb_id });
    if !is_series {
        return serde_json::to_string(&json!({ "movies": [{ "ids": ids }] })).ok();
    }
    let seasons = args
        .get("episodesBySeasonNumber")
        .and_then(Value::as_object)
        .map(|seasons| {
            seasons
                .iter()
                .filter_map(|(season, episodes)| {
                    let season = season.parse::<i64>().ok()?;
                    let episodes = episodes
                        .as_array()?
                        .iter()
                        .filter_map(Value::as_i64)
                        .map(|number| json!({ "number": number }))
                        .collect::<Vec<_>>();
                    Some(json!({ "number": season, "episodes": episodes }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::to_string(&json!({ "shows": [{ "ids": ids, "seasons": seasons }] })).ok()
}

pub(crate) fn simkl_watchlist_request_json(args_json: &str, remove: bool) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let imdb_id = args.get("imdbId")?.as_str()?.trim();
    if imdb_id.is_empty() {
        return None;
    }
    let is_series = args.get("isSeries")?.as_bool()?;
    let item = if remove {
        json!({ "ids": { "imdb": imdb_id } })
    } else {
        json!({ "ids": { "imdb": imdb_id }, "to": "plantowatch" })
    };
    serde_json::to_string(&if is_series {
        json!({ "shows": [item] })
    } else {
        json!({ "movies": [item] })
    })
    .ok()
}

pub(crate) fn trakt_sync_item_to_meta_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let item = args.get("item")?;
    let summary = item.get("movie").or_else(|| item.get("show"))?;
    let id = trakt_content_id_from_ids_json(&summary.get("ids")?.to_string())?;
    let year = summary.get("year").and_then(Value::as_i64);
    serde_json::to_string(&json!({"id":id,"name":summary.get("title").and_then(Value::as_str).filter(|name| !name.trim().is_empty()).unwrap_or_else(|| args.get("unknownName").and_then(Value::as_str).unwrap_or("Unknown")),"type":args.get("type")?.as_str()?,"poster":Value::Null,"releaseInfo":year.map(|year| year.to_string()),"released":year.map(|year| format!("{year}-01-01"))})).ok()
}

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

pub(crate) fn provider_calendar_items_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let provider = args.get("provider")?.as_str()?;
    let shows = args.get("shows").and_then(Value::as_array);
    let movies = args.get("movies").and_then(Value::as_array);
    let entries = args.get("entries").and_then(Value::as_array);
    let mut items = Vec::new();
    if provider == "anilist" {
        for entry in entries.into_iter().flatten() {
            let Some(media) = entry.get("media") else {
                continue;
            };
            let Some(next) = media.get("nextAiringEpisode") else {
                continue;
            };
            let Some(media_id) = media.get("id").and_then(Value::as_i64) else {
                continue;
            };
            let Some(episode) = next.get("episode").and_then(Value::as_i64) else {
                continue;
            };
            let Some(airing_at) = next.get("airingAt").and_then(Value::as_i64) else {
                continue;
            };
            let content_id = format!("anilist:{media_id}");
            let title = media
                .pointer("/title/english")
                .or_else(|| media.pointer("/title/romaji"));
            let Some(date_iso) =
                chrono::DateTime::from_timestamp(airing_at, 0).map(|value| value.to_rfc3339())
            else {
                continue;
            };
            items.push(json!({
                "id": format!("{content_id}:{episode}"),
                "title": title,
                "dateIso": date_iso,
                "contentId": content_id,
                "seriesId": content_id,
            }));
        }
        return serde_json::to_string(&items).ok();
    }
    for entry in shows.into_iter().flatten() {
        let Some(show) = entry.get("show") else {
            continue;
        };
        let Some(episode) = entry.get("episode") else {
            continue;
        };
        let Some(ids) = show.get("ids") else { continue };
        let Some(series_id) = ids
            .get("imdb")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                ids.get("tmdb")
                    .and_then(Value::as_i64)
                    .map(|id| format!("tmdb:{id}"))
            })
        else {
            continue;
        };
        let Some(date) = entry
            .get(if provider == "trakt" {
                "first_aired"
            } else {
                "date"
            })
            .and_then(Value::as_str)
        else {
            continue;
        };
        let season = episode.get("season").and_then(Value::as_i64);
        let number = episode
            .get(if provider == "trakt" {
                "number"
            } else {
                "episode"
            })
            .and_then(Value::as_i64);
        items.push(json!({
            "id": format!("{series_id}:{}:{}", season.unwrap_or_default(), number.unwrap_or_default()),
            "title": show.get("title"),
            "episodeTitle": episode.get("title"),
            "dateIso": date,
            "contentId": series_id,
            "seriesId": series_id,
        }));
    }
    for entry in movies.into_iter().flatten() {
        let Some(movie) = entry.get("movie") else {
            continue;
        };
        let Some(ids) = movie.get("ids") else {
            continue;
        };
        let Some(content_id) = ids
            .get("imdb")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                ids.get("tmdb")
                    .and_then(Value::as_i64)
                    .map(|id| format!("tmdb:{id}"))
            })
        else {
            continue;
        };
        let Some(date) = entry
            .get(if provider == "trakt" {
                "released"
            } else {
                "date"
            })
            .and_then(Value::as_str)
        else {
            continue;
        };
        items.push(json!({"id": content_id, "title": movie.get("title"), "dateIso": date, "contentId": content_id}));
    }
    serde_json::to_string(&items).ok()
}

pub(crate) fn provider_pagination_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let base_url = args.get("baseUrl")?.as_str()?;
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)?;
    let page = args.get("page").and_then(Value::as_i64).unwrap_or(0);
    let mut items = args
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let page_items = args
        .get("pageItems")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    items.extend(page_items.iter().cloned());
    let page_count = args
        .get("pageCount")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0);
    let response_ok = args
        .get("responseOk")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let done = !response_ok
        || (page > 0 && page_items.is_empty())
        || page >= 100
        || page_count.is_some_and(|count| page >= count)
        || (page > 0 && page_items.len() < limit as usize);
    let next_page = if page <= 0 { 1 } else { page + 1 };
    let separator = if base_url.contains('?') { '&' } else { '?' };
    serde_json::to_string(&json!({
        "items": items,
        "done": done,
        "page": next_page,
        "requestUrl": (!done).then(|| format!("{base_url}{separator}page={next_page}&limit={limit}")),
    })).ok()
}

pub(crate) fn stremio_library_mutation_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let kind = args.get("kind")?.as_str()?;
    let meta = args.get("meta").or_else(|| args.get("item"));
    let now_ms = args
        .get("nowMs")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let watched_at =
        chrono::DateTime::from_timestamp_millis(now_ms).map(|value| value.to_rfc3339());
    let item_value = |source: &Value, state: Value, extra: Value| {
        let id = source.get("id").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            return None;
        }
        let mut item = json!({
            "_id": id,
            "name": source.get("name").and_then(Value::as_str).unwrap_or(""),
            "type": source.get("type").and_then(Value::as_str).unwrap_or("movie"),
            "poster": source.get("poster"), "background": source.get("background"), "logo": source.get("logo"),
            "state": state,
        });
        if let (Some(target), Some(fields)) = (item.as_object_mut(), extra.as_object()) {
            target.extend(fields.clone());
        }
        Some(item)
    };
    let changes: Vec<Value> = match kind {
        "watchlist" => {
            let source = meta?;
            let removed = args.get("command").and_then(Value::as_str) == Some("remove");
            item_value(source, json!({"lastWatched": null, "timeOffset": 0, "duration": 0, "videoId": null, "timesWatched": 0, "flaggedWatched": 0}), json!({"removed": if removed { 1 } else { 0 }})).into_iter().collect()
        }
        "progress" => {
            let source = meta?;
            let progress = args.get("progress")?;
            let last_watched = progress
                .get("lastWatched")
                .and_then(Value::as_i64)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|value| value.to_rfc3339());
            item_value(source, json!({
                "lastWatched": last_watched,
                "timeOffset": progress.get("positionSeconds").and_then(Value::as_f64).unwrap_or_default().max(0.0).round() as i64,
                "duration": progress.get("durationSeconds").and_then(Value::as_f64).unwrap_or_default().max(0.0).round() as i64,
                "videoId": progress.get("videoId"),
            }), json!({})).into_iter().collect()
        }
        "watched" => {
            let watched = args
                .get("watched")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let timestamp = watched.then_some(watched_at).flatten();
            let episodes = args
                .get("episodes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if episodes.is_empty() {
                meta.and_then(|source| item_value(source, json!({"lastWatched": timestamp, "timeOffset": 0, "duration": 0, "videoId": null, "timesWatched": if watched { 1 } else { 0 }, "flaggedWatched": if watched { 1 } else { 0 }}), json!({"lastWatched": timestamp}))).into_iter().collect()
            } else {
                episodes.iter().filter_map(|episode| {
                    let content_id = episode.get("contentId").and_then(Value::as_str).unwrap_or("");
                    let video_id = episode.get("videoId").and_then(Value::as_str).filter(|value| !value.is_empty()).map(str::to_string)
                        .unwrap_or_else(|| format!("{content_id}:{}:{}", episode.get("season").and_then(Value::as_i64).unwrap_or_default(), episode.get("episode").and_then(Value::as_i64).unwrap_or_default()));
                    Some(json!({
                        "_id": video_id, "name": episode.get("title").and_then(Value::as_str).or_else(|| meta.and_then(|value| value.get("name")).and_then(Value::as_str)).unwrap_or(""),
                        "type": episode.get("contentType"), "poster": meta.and_then(|value| value.get("poster")), "background": meta.and_then(|value| value.get("background")), "logo": meta.and_then(|value| value.get("logo")),
                        "state": {"lastWatched": timestamp, "timeOffset": 0, "duration": 0, "videoId": video_id, "timesWatched": if watched { 1 } else { 0 }, "flaggedWatched": if watched { 1 } else { 0 }},
                        "lastWatched": timestamp,
                    }))
                }).collect()
            }
        }
        _ => return None,
    };
    serde_json::to_string(&changes).ok()
}

const TRAKT_API_BASE_URL: &str = "https://api.trakt.tv";

pub(crate) fn promote_external_progress_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let source = args.get("source")?.as_str()?;
    let mut progress = args.get("progress")?.as_object()?.clone();
    let mut promotions = Vec::new();
    for item in args.get("items")?.as_array()? {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("");
        let video_id = item
            .get("lastVideoId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let duration = item.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
        let offset = item
            .get("timeOffset")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let saved_at = item.get("savedAt").and_then(Value::as_str).unwrap_or("");
        let Some(saved_ms) = chrono::DateTime::parse_from_rfc3339(saved_at)
            .ok()
            .map(|value| value.timestamp_millis())
        else {
            continue;
        };
        if id.is_empty() || video_id.is_empty() || duration <= 0.0 {
            continue;
        }
        let existing_ms = progress
            .get(id)
            .and_then(|value| value.get("savedAt"))
            .and_then(Value::as_str)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.timestamp_millis())
            .unwrap_or(0);
        if existing_ms >= saved_ms {
            continue;
        }
        let existing = progress
            .get(id)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let existing_meta = existing
            .get("meta")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let item_fields: Map<String, Value> = item
            .as_object()?
            .iter()
            .filter(|(_, value)| !value.is_null())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let mut next = existing;
        next.extend(item_fields.clone());
        let mut merged_meta = existing_meta;
        merged_meta.extend(item_fields);
        next.insert("meta".to_string(), Value::Object(merged_meta));
        next.insert("source".to_string(), Value::String(source.to_string()));
        next.insert("savedAt".to_string(), Value::String(saved_at.to_string()));
        progress.insert(id.to_string(), Value::Object(next));
        let content_type = item.get("type").and_then(Value::as_str).unwrap_or("movie");
        let season = item.get("lastEpisodeSeason").and_then(Value::as_i64);
        let episode_number = item.get("lastEpisodeNumber").and_then(Value::as_i64);
        promotions.push(json!({
            "item": item,
            "externalProgress": {
                "contentId": id,
                "contentType": content_type,
                "videoId": video_id,
                "positionSeconds": offset,
                "durationSeconds": duration,
                "lastWatched": saved_ms,
                "season": season,
                "episode": episode_number,
            },
            "meta": {"id": id, "type": content_type, "name": item.get("name").and_then(Value::as_str).unwrap_or("")},
            "episode": match (season, episode_number) {
                (Some(season), Some(episode)) => json!({"id": video_id, "season": season, "episode": episode, "number": episode}),
                _ => Value::Null,
            },
            "scrobbleTrakt": source != "trakt",
            "scrobbleSimkl": source != "simkl",
        }));
    }
    serde_json::to_string(&json!({"progress": progress, "promotions": promotions})).ok()
}

pub(crate) fn external_provider_action_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let kind = args.get("kind")?.as_str()?;
    if kind == "sync" {
        let provider = args
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("trakt")
            .to_ascii_lowercase();
        let supported = matches!(
            provider.as_str(),
            "anilist" | "simkl" | "trakt" | "stremio" | "nuvio"
        );
        return Some(json!({
            "provider": provider,
            "supported": supported,
            "error": (!supported).then(|| format!("Unsupported external sync provider: {provider}")),
        }).to_string());
    }
    let profile = args.get("profile").filter(|value| !value.is_null())?;
    let now_ms = args.get("nowMs").and_then(Value::as_i64).unwrap_or(0);
    let has = |key: &str| {
        profile
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    let trakt = has("traktAccessToken")
        && profile
            .get("traktTokenExpiresAt")
            .and_then(Value::as_i64)
            .is_none_or(|expires| now_ms / 1000 <= expires);
    let simkl = has("simklAccessToken");
    let anilist = has("anilistAccessToken");
    let stremio = has("stremioAuthKey");
    let nuvio = has("nuvioAccessToken");
    match kind {
        "markWatched" => {
            let watched = args.get("watched").and_then(Value::as_bool).unwrap_or(true);
            let episode_infos: Vec<Value> = match args.get("episodeInfo") {
                Some(Value::Array(values)) => values.clone(),
                Some(value) if !value.is_null() => vec![value.clone()],
                _ => Vec::new(),
            }.into_iter().filter(|info| info.get("contentId").and_then(Value::as_str).is_some_and(|id| !id.is_empty())).collect();
            let meta = args.get("meta").cloned().unwrap_or(Value::Null);
            let video_ids = args.get("videoIds").and_then(Value::as_array).cloned().unwrap_or_default();
            let fallback_id = meta.get("id").and_then(Value::as_str).or_else(|| video_ids.first().and_then(Value::as_str)).unwrap_or("");
            let watched_keys: Vec<Value> = if episode_infos.is_empty() {
                (!fallback_id.is_empty()).then(|| json!({"content_id": fallback_id, "season": Value::Null, "episode": Value::Null})).into_iter().collect()
            } else {
                episode_infos.iter().map(|info| json!({"content_id": info.get("contentId"), "season": info.get("season"), "episode": info.get("episode")})).collect()
            };
            let history_items: Vec<Value> = episode_infos.iter().map(|info| json!({
                "content_id": info.get("contentId"), "content_type": info.get("contentType"), "title": info.get("title").and_then(Value::as_str).unwrap_or(""),
                "season": info.get("season"), "episode": info.get("episode"), "watched_at": now_ms,
            })).collect();
            let progress_entry = args.get("progressInfo").filter(|value| value.get("contentId").is_some() && value.get("videoId").is_some() && value.get("durationSeconds").and_then(Value::as_f64).unwrap_or(0.0) > 0.0).map(progress_to_nuvio);
            let anime_episode = episode_infos.last().cloned().unwrap_or(Value::Null);
            Some(json!({
                "trakt": trakt, "simkl": simkl, "anilist": anilist && watched, "stremio": stremio, "nuvio": nuvio,
                "animeEpisode": anime_episode, "animeProgressEpisode": args.pointer("/progressInfo/episode").cloned().or_else(|| anime_episode.get("episode").cloned()),
                "episodes": episode_infos, "watchedKeys": watched_keys, "historyItems": history_items, "progressEntry": progress_entry,
            }).to_string())
        }
        "watchlist" => {
            let command = args.get("command").and_then(Value::as_str).unwrap_or("add");
            Some(json!({"trakt": trakt, "simkl": simkl && command == "add", "anilist": anilist, "stremio": stremio, "nuvio": nuvio}).to_string())
        }
        "progress" => {
            let progress = args.get("progress")?;
            let valid = progress.get("durationSeconds").and_then(Value::as_f64).unwrap_or(0.0) > 0.0;
            Some(json!({"stremio": stremio && valid, "nuvio": nuvio && valid, "progressEntry": valid.then(|| progress_to_nuvio(progress))}).to_string())
        }
        "status" => Some(json!({"anilist": anilist}).to_string()),
        "dropProgress" => Some(json!({"dropTrakt": args.pointer("/item/reason").and_then(Value::as_str).is_some_and(|value| value.eq_ignore_ascii_case("trakt"))}).to_string()),
        _ => None,
    }
}

fn progress_to_nuvio(progress: &Value) -> Value {
    json!({
        "content_id": progress.get("contentId"), "content_type": progress.get("contentType"), "video_id": progress.get("videoId"),
        "position": (progress.get("positionSeconds").and_then(Value::as_f64).unwrap_or(0.0) * 1000.0).round() as i64,
        "duration": (progress.get("durationSeconds").and_then(Value::as_f64).unwrap_or(0.0) * 1000.0).round() as i64,
        "last_watched": progress.get("lastWatched"), "season": progress.get("season"), "episode": progress.get("episode")
    })
}

pub(crate) fn trakt_has_client(api_key: &str) -> bool {
    !api_key.trim().is_empty()
}

pub(crate) fn trakt_bearer(token: &str) -> String {
    format!("Bearer {token}")
}

pub(crate) fn trakt_scrobble_url(action: &str) -> Option<String> {
    matches!(action.trim(), "start" | "pause" | "stop")
        .then(|| format!("{TRAKT_API_BASE_URL}/scrobble/{}", action.trim()))
}

pub(crate) fn trakt_playback_url(content_type: Option<&str>) -> Option<String> {
    let suffix = match content_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None => None,
        Some("movie" | "movies") => Some("movies"),
        Some("series" | "show" | "shows" | "episode" | "episodes") => Some("episodes"),
        Some(_) => return None,
    };
    Some(match suffix {
        Some(suffix) => format!("{TRAKT_API_BASE_URL}/sync/playback/{suffix}"),
        None => format!("{TRAKT_API_BASE_URL}/sync/playback"),
    })
}

pub(crate) fn trakt_token_expires_at(created_at_seconds: i64, expires_in_seconds: i64) -> i64 {
    let refresh_buffer_seconds = 5 * 60;
    let effective_expires_in = (expires_in_seconds - refresh_buffer_seconds).max(0);
    created_at_seconds + effective_expires_in
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

pub(super) fn trakt_ids_from_content_id_json(raw_id: &str) -> Option<String> {
    let imdb = imdb_regex().find(raw_id).map(|m| m.as_str().to_string());
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

pub(super) fn trakt_id_from_source(source: &Value) -> Option<String> {
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

pub(crate) fn trakt_playback_delete_ids_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let content_id = args.get("contentId")?.as_str()?;
    let ids = args
        .get("items")?
        .as_array()?
        .iter()
        .filter_map(|item| {
            let source = item.get("show").or_else(|| item.get("movie"))?;
            (trakt_id_from_source(source).as_deref() == Some(content_id))
                .then(|| item.get("id")?.as_i64())
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&ids).ok()
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
        let season = ep.get("season").and_then(Value::as_i64).unwrap_or(0);
        let number = ep.get("number").and_then(Value::as_i64).unwrap_or(0);
        format!("{id}:{season}:{number}")
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
        let Some(movie) = entry.get("movie") else {
            continue;
        };
        let Some(id) = trakt_id_from_source(movie) else {
            continue;
        };
        let name = movie.get("title").and_then(Value::as_str).unwrap_or("");
        items.push(json!({ "id": id, "name": name, "type": "movie", "source": "trakt" }));
    }
    for entry in &shows {
        let Some(show) = entry.get("show") else {
            continue;
        };
        let Some(id) = trakt_id_from_source(show) else {
            continue;
        };
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
        if let Some(id) = entry.get("movie").and_then(trakt_id_from_source) {
            ids.insert(id, Value::Bool(true));
        }
    }
    for entry in &shows {
        let show_id = match entry.get("show").and_then(trakt_id_from_source) {
            Some(id) => id,
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
                    ids.insert(format!("{show_id}:{s_num}:{e_num}"), Value::Bool(true));
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

#[derive(serde::Deserialize)]
struct TimestampedLocalItem {
    id: String,
    #[serde(default)]
    active: bool,
    #[serde(rename = "updatedAt", default)]
    updated_at: i64,
}

#[derive(serde::Deserialize)]
struct TimestampedRemoteItem {
    id: String,
    #[serde(rename = "updatedAt", default)]
    updated_at: i64,
}

fn merge_timestamped_membership(local_json: &str, remote_json: &str) -> String {
    let local: Vec<TimestampedLocalItem> = serde_json::from_str(local_json).unwrap_or_default();
    let remote: Vec<TimestampedRemoteItem> = serde_json::from_str(remote_json).unwrap_or_default();

    let local_by_id: std::collections::HashMap<&str, &TimestampedLocalItem> =
        local.iter().map(|item| (item.id.as_str(), item)).collect();
    let remote_ids: std::collections::HashSet<&str> =
        remote.iter().map(|item| item.id.as_str()).collect();

    let mut apply_local_add: Vec<String> = Vec::new();
    let mut push_remote_add: Vec<String> = Vec::new();
    let mut push_remote_remove: Vec<String> = Vec::new();

    for remote_item in &remote {
        match local_by_id.get(remote_item.id.as_str()) {
            None => apply_local_add.push(remote_item.id.clone()),
            Some(local_item) if !local_item.active => {
                if local_item.updated_at >= remote_item.updated_at {
                    push_remote_remove.push(remote_item.id.clone());
                } else {
                    apply_local_add.push(remote_item.id.clone());
                }
            }
            Some(_) => {}
        }
    }
    for local_item in &local {
        if local_item.active && !remote_ids.contains(local_item.id.as_str()) {
            push_remote_add.push(local_item.id.clone());
        }
    }

    json!({
        "toApplyLocal": { "add": apply_local_add },
        "toPushRemote": { "add": push_remote_add, "remove": push_remote_remove }
    })
    .to_string()
}

pub(crate) fn merge_watchlist_timestamped_json(local_json: &str, remote_json: &str) -> String {
    merge_timestamped_membership(local_json, remote_json)
}

pub(crate) fn merge_watched_timestamped_json(local_json: &str, remote_json: &str) -> String {
    merge_timestamped_membership(local_json, remote_json)
}

fn item_id(item: &Value) -> String {
    item.get("id")
        .or_else(|| item.get("_id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub(super) fn saved_at_ms(item: &Value) -> i64 {
    item.get("savedAt")
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt: chrono::DateTime<chrono::FixedOffset>| dt.timestamp_millis())
        .unwrap_or(0)
}

fn episode_rank(item: &Value) -> Option<(i64, i64)> {
    let season = item.get("lastEpisodeSeason").and_then(Value::as_i64)?;
    let number = item.get("lastEpisodeNumber").and_then(Value::as_i64)?;
    Some((season, number))
}

pub(super) fn ranked_winner(
    a: &Value,
    a_time: i64,
    b: &Value,
    b_time: i64,
    ranking_mode: Option<&str>,
) -> bool {
    if ranking_mode == Some("most_recent_episode") {
        if let (Some(ra), Some(rb)) = (episode_rank(a), episode_rank(b)) {
            if ra != rb {
                return ra > rb;
            }
        }
    }
    a_time >= b_time
}

pub(crate) fn merge_continue_watching_lists_json(
    local_json: &str,
    external_json: &str,
    progress_json: &str,
    source_of_truth: Option<&str>,
    ranking_mode: Option<&str>,
) -> Option<String> {
    let local: Vec<Value> = serde_json::from_str(local_json).unwrap_or_default();
    let external: Vec<Value> = serde_json::from_str(external_json).unwrap_or_default();
    let progress: serde_json::Map<String, Value> =
        serde_json::from_str(progress_json).unwrap_or_default();

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
        let local_item = local_by_id.get(&id).copied();
        let local_time = local_saved_at_from_progress(&progress, &id);
        let ext_time = saved_at_ms(ext_item);

        let local_wins = if let Some(local_item) = local_item {
            let local_source = local_item
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("local");
            if source_of_truth.is_some() && source_of_truth == Some(local_source) {
                true
            } else if source_of_truth.is_some()
                && source_of_truth == ext_item.get("reason").and_then(Value::as_str)
            {
                false
            } else {
                ranked_winner(local_item, local_time, ext_item, ext_time, ranking_mode)
            }
        } else {
            false
        };

        if local_wins {
            merged.push(local_item.unwrap().clone());
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

    merged.sort_by(|a, b| saved_at_ms(b).cmp(&saved_at_ms(a)));

    serde_json::to_string(&merged).ok()
}

mod provider_mappers;

pub(crate) use provider_mappers::{
    replace_external_continue_watching_json, simkl_lookup_id_for_type,
    simkl_mark_watched_body_json, simkl_match_episode_json, simkl_recommendation_candidates_json,
    simkl_recommendation_to_meta_json, simkl_watched_to_ids_json, simkl_watching_to_items_json,
    simkl_watchlist_body_json, simkl_watchlist_to_items_json, trakt_mark_watched_body_json,
    trakt_playback_items_dedup_json, trakt_related_items_to_metas_json, trakt_related_lookup_slug,
};
mod anilist;

pub(crate) use anilist::{
    anilist_entries_to_sync, anilist_media_list_status,
    anilist_save_media_list_entry_variables_json, anilist_search_best_match_json,
    extract_anilist_id_from_links, merge_library_items_by_id,
};
#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_scrobble;
    use serde_json::Value;

    #[test]
    fn replace_external_continue_watching_sorts_by_saved_at_descending() {
        let items = json!([
            {"id": "tt1", "reason": "Nuvio", "timeOffset": 100, "duration": 1000, "savedAt": "2026-07-16T16:18:15Z"},
            {"id": "tt2", "reason": "Nuvio", "timeOffset": 100, "duration": 1000, "savedAt": "2026-07-18T22:15:23Z"},
            {"id": "tt3", "reason": "Nuvio", "timeOffset": 100, "duration": 1000, "savedAt": "2026-07-17T19:11:51Z"},
        ]);
        let result = replace_external_continue_watching_json(
            "[]",
            Some("Nuvio"),
            &items.to_string(),
            None,
            None,
        );
        let parsed: Vec<Value> = serde_json::from_str(&result).unwrap();
        let ids: Vec<&str> = parsed.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["tt2", "tt3", "tt1"]);
    }

    #[test]
    fn merge_continue_watching_sorts_the_combined_result_by_saved_at_descending() {
        let local = json!([
            {"id": "tt1", "savedAt": "2026-07-16T16:18:15Z"},
            {"id": "tt3", "savedAt": "2026-07-17T19:11:51Z"},
        ]);
        let external = json!([
            {"id": "tt2", "savedAt": "2026-07-18T22:15:23Z"},
        ]);
        let result = merge_continue_watching_lists_json(
            &local.to_string(),
            &external.to_string(),
            "{}",
            None,
            None,
        )
        .unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&result).unwrap();
        let ids: Vec<&str> = parsed.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["tt2", "tt3", "tt1"]);
    }

    #[test]
    fn extracts_anilist_id_from_link_url() {
        let meta = json!({"links": [{"name": "AniList", "category": "other", "url": "https://anilist.co/anime/1535"}]});
        assert_eq!(extract_anilist_id_from_links(&meta), Some(1535));
    }

    #[test]
    fn extract_anilist_id_returns_none_without_matching_link() {
        let meta = json!({"links": [{"name": "IMDb", "category": "other", "url": "https://imdb.com/title/tt1"}]});
        assert_eq!(extract_anilist_id_from_links(&meta), None);
    }

    #[test]
    fn extract_anilist_id_skips_malformed_link_and_uses_next_match() {
        let meta = json!({"links": [
            {"name": "Bad", "category": "other", "url": "https://anilist.co/anime/"},
            {"name": "Good", "category": "other", "url": "https://anilist.co/anime/1535"},
        ]});
        assert_eq!(extract_anilist_id_from_links(&meta), Some(1535));
    }

    #[test]
    fn search_match_prefers_exact_title_within_year_tolerance() {
        let args = json!({
            "meta": {"name": "Attack on Titan", "year": 2013},
            "candidates": [
                {"id": 1, "seasonYear": 2020, "title": {"romaji": "Something Else"}},
                {"id": 2, "seasonYear": 2013, "title": {"english": "Attack on Titan"}},
            ],
        });
        let result = anilist_search_best_match_json(&args.to_string()).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["anilistId"], 2);
        assert_eq!(parsed["confidence"], "title-year");
    }

    #[test]
    fn search_match_falls_back_to_year_only_when_no_title_matches() {
        let args = json!({
            "meta": {"name": "Some Show", "year": 2013},
            "candidates": [
                {"id": 3, "seasonYear": 2013, "title": {"romaji": "Different Name"}},
            ],
        });
        let result = anilist_search_best_match_json(&args.to_string()).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["anilistId"], 3);
    }

    #[test]
    fn media_list_status_completes_when_progress_reaches_total() {
        assert_eq!(anilist_media_list_status(12, 12), "COMPLETED");
        assert_eq!(anilist_media_list_status(12, 5), "CURRENT");
        assert_eq!(anilist_media_list_status(0, 5), "CURRENT");
    }

    #[test]
    fn anilist_current_entries_build_progress_and_watched_keys() {
        let entries = r#"[
            {"status":"CURRENT","progress":3,"updatedAt":1700000000,"media":{"id":5,"title":{"romaji":"Show"},"episodes":12}},
            {"status":"PLANNING","media":{"id":6,"title":{"english":"Other"}}}
        ]"#;
        let plan = anilist_entries_to_sync(
            serde_json::from_str::<Vec<Value>>(entries)
                .unwrap()
                .as_slice(),
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
        assert_eq!(plan["watchlist"][0]["updatedAtMs"], json!(0));
    }

    #[test]
    fn anilist_planning_entry_carries_updated_at_ms_when_present() {
        let entries = r#"[
            {"status":"PLANNING","updatedAt":1700000000,"media":{"id":9,"title":{"romaji":"Planned"}}}
        ]"#;
        let plan = anilist_entries_to_sync(
            serde_json::from_str::<Vec<Value>>(entries)
                .unwrap()
                .as_slice(),
            0,
        );
        assert_eq!(plan["watchlist"][0]["updatedAtMs"], json!(1700000000000i64));
    }

    #[test]
    fn merge_by_id_overlays_incoming_fields_onto_local_items() {
        let local: Vec<Value> = serde_json::from_str(
            r#"[{"id":"a","name":"Old","poster":"p"},{"id":"b","name":"Keep"}]"#,
        )
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
    fn trakt_token_expiry_stays_in_epoch_seconds() {
        assert_eq!(trakt_token_expires_at(1_700_000_000, 3_600), 1_700_003_300);
    }

    #[test]
    fn trakt_urls_accept_only_supported_routes() {
        assert_eq!(
            trakt_scrobble_url("pause").as_deref(),
            Some("https://api.trakt.tv/scrobble/pause")
        );
        assert_eq!(trakt_scrobble_url("delete"), None);
        assert_eq!(
            trakt_playback_url(Some("series")).as_deref(),
            Some("https://api.trakt.tv/sync/playback/episodes")
        );
        assert_eq!(trakt_playback_url(Some("unknown")), None);
    }

    #[test]
    fn external_sync_wire_fixtures_preserve_provider_contracts() {
        let trakt_input: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/trakt_scrobble_plan_input.json"
        ))
        .unwrap();
        let trakt_expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/trakt_scrobble_plan_expected.json"
        ))
        .unwrap();
        let trakt_actual: Value = serde_json::from_str(
            &player_scrobble::trakt_scrobble_plan_json(
                &trakt_input["ids"].to_string(),
                trakt_input["isEpisode"].as_bool().unwrap(),
                None,
                None,
                trakt_input["timePosSec"].as_f64().unwrap(),
                trakt_input["durationSec"].as_f64().unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(trakt_actual, trakt_expected);

        let simkl_input =
            include_str!("../tests/fixtures/external_sync/simkl_mark_watched_input.json");
        let simkl_expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/simkl_mark_watched_expected.json"
        ))
        .unwrap();
        let simkl_actual: Value =
            serde_json::from_str(&simkl_mark_watched_body_json(simkl_input).unwrap()).unwrap();
        assert_eq!(simkl_actual, simkl_expected);

        let trakt_playback_expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/trakt_playback_expected.json"
        ))
        .unwrap();
        let trakt_playback_actual: Value = serde_json::from_str(
            &trakt_playback_items_to_library_json(include_str!(
                "../tests/fixtures/external_sync/trakt_playback_response.json"
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(trakt_playback_actual, trakt_playback_expected);

        let simkl_response: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/simkl_watched_response.json"
        ))
        .unwrap();
        let simkl_watched_expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/simkl_watched_expected.json"
        ))
        .unwrap();
        let simkl_watched_actual: Value = serde_json::from_str(
            &simkl_watched_to_ids_json(
                &simkl_response["shows"].to_string(),
                &simkl_response["movies"].to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(simkl_watched_actual, simkl_watched_expected);
    }

    #[test]
    fn trakt_playback_tmdb_show_keeps_a_resolvable_episode_id() {
        let item = json!({
            "progress": 50.0,
            "paused_at": "2026-07-21T00:00:00.000Z",
            "show": {"title": "Show", "runtime": 45, "ids": {"tmdb": 42}},
            "episode": {"season": 1, "number": 2, "runtime": 45}
        });
        let result = trakt_playback_item_to_library(&item).expect("playback item");
        assert_eq!(result["id"], "tmdb:42");
        assert_eq!(result["lastVideoId"], "tmdb:42:1:2");
    }

    #[test]
    fn external_list_mappers_skip_invalid_records_and_keep_valid_ones() {
        let trakt: Vec<Value> = serde_json::from_str(
            &trakt_watchlist_to_items_json(
                r#"[{"movie":{"title":"Valid","ids":{"tmdb":7}}},{"movie":{"title":"Invalid","ids":{}}}]"#,
                "[]",
            )
            .expect("trakt items"),
        )
        .unwrap();
        assert_eq!(trakt.len(), 1);
        assert_eq!(trakt[0]["id"], "tmdb:7");

        let simkl: Vec<Value> = serde_json::from_str(
            &simkl_watchlist_to_items_json(
                r#"[{"show":{"title":"Valid","ids":{"imdb":"tt7"}}},{"show":{"title":"Invalid","ids":{}}}]"#,
                "[]",
            )
            .expect("simkl items"),
        )
        .unwrap();
        assert_eq!(simkl.len(), 1);
        assert_eq!(simkl[0]["id"], "tt7");
    }

    #[test]
    fn watched_mappers_retain_tmdb_only_records() {
        let trakt: Value = serde_json::from_str(
            &trakt_watched_to_ids_json(r#"[{"movie":{"ids":{"tmdb":7}}}]"#, "[]")
                .expect("trakt watched"),
        )
        .unwrap();
        assert_eq!(trakt["tmdb:7"], Value::Bool(true));

        let simkl: Value = serde_json::from_str(
            &simkl_watched_to_ids_json("[]", r#"[{"movie":{"ids":{"tmdb":8}}}]"#)
                .expect("simkl watched"),
        )
        .unwrap();
        assert_eq!(simkl["tmdb:8"], Value::Bool(true));
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

    #[test]
    fn timestamped_merge_pushes_removal_when_local_is_newer() {
        let local = json!([{"id": "a", "active": false, "updatedAt": 2000}]).to_string();
        let remote = json!([{"id": "a", "updatedAt": 1000}]).to_string();
        let result: Value =
            serde_json::from_str(&merge_watchlist_timestamped_json(&local, &remote)).unwrap();
        assert_eq!(result["toPushRemote"]["remove"], json!(["a"]));
        assert_eq!(result["toApplyLocal"]["add"], json!([]));
    }

    #[test]
    fn timestamped_merge_reapplies_locally_when_remote_is_newer() {
        let local = json!([{"id": "a", "active": false, "updatedAt": 1000}]).to_string();
        let remote = json!([{"id": "a", "updatedAt": 2000}]).to_string();
        let result: Value =
            serde_json::from_str(&merge_watchlist_timestamped_json(&local, &remote)).unwrap();
        assert_eq!(result["toApplyLocal"]["add"], json!(["a"]));
        assert_eq!(result["toPushRemote"]["remove"], json!([]));
    }

    #[test]
    fn timestamped_merge_pushes_local_only_additions() {
        let local = json!([{"id": "a", "active": true, "updatedAt": 1000}]).to_string();
        let remote = json!([]).to_string();
        let result: Value =
            serde_json::from_str(&merge_watchlist_timestamped_json(&local, &remote)).unwrap();
        assert_eq!(result["toPushRemote"]["add"], json!(["a"]));
    }

    #[test]
    fn anilist_save_media_list_entry_variables_parses_media_id() {
        let json =
            anilist_save_media_list_entry_variables_json("anilist:5", "PLANNING", None).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["mediaId"], json!(5));
        assert_eq!(value["status"], json!("PLANNING"));
        assert!(value.get("progress").is_none());
    }

    #[test]
    fn anilist_save_media_list_entry_variables_includes_progress_when_given() {
        let json = anilist_save_media_list_entry_variables_json("anilist:5", "COMPLETED", Some(12))
            .unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["progress"], json!(12));
    }

    #[test]
    fn anilist_save_media_list_entry_variables_rejects_non_anilist_ids() {
        assert_eq!(
            anilist_save_media_list_entry_variables_json("tt1234567", "PLANNING", None),
            None
        );
    }

    #[test]
    fn timestamped_merge_imports_remote_only_additions() {
        let local = json!([]).to_string();
        let remote = json!([{"id": "a", "updatedAt": 1000}]).to_string();
        let result: Value =
            serde_json::from_str(&merge_watchlist_timestamped_json(&local, &remote)).unwrap();
        assert_eq!(result["toApplyLocal"]["add"], json!(["a"]));
    }

    #[test]
    fn mal_sync_policy_maps_auth_and_episode_updates() {
        assert_eq!(
            external_sync_response_action("mal", 401),
            "refresh_credentials"
        );
        assert_eq!(
            external_sync_response_action("simkl", 401),
            "clear_credentials"
        );
        assert_eq!(
            external_sync_refresh_retry_action(Some(401)),
            "clear_credentials"
        );
        let watched = mal_list_update_json(
            &json!({
                "meta": { "id": "mal:42", "type": "series", "episodesCount": 12 },
                "episodes": [{ "number": 12 }],
            })
            .to_string(),
            true,
        )
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap();
        assert_eq!(watched["status"], "completed");
    }

    #[test]
    fn simkl_request_policy_builds_series_history_and_watchlist_removal() {
        let history = simkl_history_request_json(
            &json!({
                "imdbId": "tt1",
                "isSeries": true,
                "episodesBySeasonNumber": { "2": [3, 4] },
            })
            .to_string(),
        )
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap();
        assert_eq!(history["shows"][0]["seasons"][0]["number"], 2);
        assert_eq!(
            history["shows"][0]["seasons"][0]["episodes"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );

        let removal = simkl_watchlist_request_json(
            &json!({ "imdbId": "tt1", "isSeries": false }).to_string(),
            true,
        )
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap();
        assert!(removal["movies"][0].get("to").is_none());
    }

    #[test]
    fn trakt_sync_item_policy_normalizes_identity_and_release_date() {
        let meta = trakt_sync_item_to_meta_json(
            &json!({
                "item": { "show": { "title": "Show", "year": 2025, "ids": { "imdb": "tt1" } } },
                "type": "series",
                "unknownName": "Unknown",
            })
            .to_string(),
        )
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap();
        assert_eq!(meta["id"], "tt1");
        assert_eq!(meta["released"], "2025-01-01");
    }

    #[test]
    fn trakt_playback_deletion_matches_shared_content_identity() {
        let ids: Value = serde_json::from_str(&trakt_playback_delete_ids_json(&json!({
            "contentId":"tmdb:42",
            "items":[{"id":1,"show":{"ids":{"tmdb":42}}},{"id":2,"movie":{"ids":{"imdb":"tt1"}}}],
        }).to_string()).unwrap()).unwrap();
        assert_eq!(ids, json!([1]));
    }
}
