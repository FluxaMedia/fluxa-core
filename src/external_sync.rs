use crate::content_identity::{
    base_content_id, imdb_regex, parse_episode_locator, parse_video_id_json,
};
use serde_json::{json, Map, Value};

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

fn saved_at_ms(item: &Value) -> i64 {
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

fn ranked_winner(
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
    source_of_truth: Option<&str>,
    ranking_mode: Option<&str>,
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

pub(crate) fn simkl_mark_watched_body_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let video_ids = args.get("videoIds")?.as_array()?;
    let meta_type = args
        .pointer("/meta/type")
        .and_then(Value::as_str)
        .unwrap_or("movie");
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
            movies.push(json!({"ids": ids, "watched_at": "now"}));
        }
    }
    let show_values = shows.into_values().map(|(ids, seasons)| {
        if seasons.is_empty() { return json!({"ids": ids}); }
        json!({"ids": ids, "seasons": seasons.into_iter().map(|(number, mut episodes)| {
            episodes.sort_unstable(); episodes.dedup();
            json!({"number": number, "episodes": episodes.into_iter().map(|number| json!({"number": number})).collect::<Vec<_>>()})
        }).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
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
    let entry = json!({"ids": ids, "to": "plantowatch"});
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

fn mark_watched_through(
    watched: &mut serde_json::Map<String, Value>,
    watched_at_ms: &mut serde_json::Map<String, Value>,
    id: &str,
    through: i64,
    updated_at_ms: i64,
) {
    for ep in 1..=through.max(0) {
        let key = format!("{id}:1:{ep}");
        watched.insert(key.clone(), Value::Bool(true));
        watched_at_ms.insert(key, json!(updated_at_ms));
    }
}

pub(crate) fn anilist_media_id_from_content_id(content_id: &str) -> Option<i64> {
    content_id.strip_prefix("anilist:")?.parse::<i64>().ok()
}

pub(crate) fn anilist_save_media_list_entry_variables_json(
    content_id: &str,
    status: &str,
    progress: Option<i64>,
) -> Option<String> {
    let media_id = anilist_media_id_from_content_id(content_id)?;
    let mut variables = Map::new();
    variables.insert("mediaId".to_string(), json!(media_id));
    variables.insert("status".to_string(), json!(status));
    if let Some(progress) = progress {
        variables.insert("progress".to_string(), json!(progress.max(0)));
    }
    serde_json::to_string(&Value::Object(variables)).ok()
}

pub(crate) fn anilist_entries_to_sync(entries: &[Value], now_ms: i64) -> Value {
    let mut watchlist: Vec<Value> = Vec::new();
    let mut completed: Vec<Value> = Vec::new();
    let mut dropped: Vec<Value> = Vec::new();
    let mut watching: Vec<Value> = Vec::new();
    let mut watched = Map::new();
    let mut watched_at_ms = Map::new();
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
        let updated_at_ms = entry.updated_at.map(|s| s * 1000).unwrap_or(now_ms);

        match status.as_str() {
            "PLANNING" => {
                let mut it = item.clone();
                it.insert("inWatchlist".to_string(), Value::Bool(true));
                it.insert("updatedAtMs".to_string(), json!(updated_at_ms));
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
                mark_watched_through(
                    &mut watched,
                    &mut watched_at_ms,
                    &id,
                    through,
                    updated_at_ms,
                );
            }
            "DROPPED" | "PAUSED" => {
                let mut it = item.clone();
                it.insert("statusChangedAt".to_string(), json!(updated_at));
                dropped.push(Value::Object(it));
                mark_watched_through(
                    &mut watched,
                    &mut watched_at_ms,
                    &id,
                    progress_episode,
                    updated_at_ms,
                );
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
                mark_watched_through(
                    &mut watched,
                    &mut watched_at_ms,
                    &id,
                    progress_episode,
                    updated_at_ms,
                );
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
        "watchedUpdatedAtMs": Value::Object(watched_at_ms),
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
}
