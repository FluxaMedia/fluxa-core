use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

pub(crate) fn remote_collection_request_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let source = request.get("source")?;
    let provider = source.get("provider").and_then(Value::as_str)?;
    let page = request
        .get("page")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    if provider == "trakt" {
        let list_id = source.get("traktListId").and_then(Value::as_i64)?;
        let client_id = request
            .get("clientId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())?;
        let media_type = if source
            .get("mediaType")
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("TV"))
        {
            "show"
        } else {
            "movie"
        };
        let mut params = serde_json::Map::from_iter([
            ("extended".into(), json!("full,images")),
            ("page".into(), json!(page)),
            ("limit".into(), json!(50)),
        ]);
        for (input, output) in [("sortBy", "sort_by"), ("sortHow", "sort_how")] {
            if let Some(value) = source
                .get(input)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                params.insert(output.into(), json!(value));
            }
        }
        return serde_json::to_string(&json!({
            "url": format!("https://api.trakt.tv/lists/{list_id}/items/{media_type}"), "params": params,
            "headers": {"trakt-api-version": "2", "trakt-api-key": client_id}, "responseKind": "trakt", "requestedType": if media_type == "show" { "series" } else { "movie" }
        })).ok();
    }
    if provider != "tmdb" {
        return None;
    }
    let api_key = request
        .get("apiKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let language = request
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("en")
        .replace('_', "-");
    let source_type = source
        .get("tmdbSourceType")
        .and_then(Value::as_str)
        .unwrap_or("DISCOVER");
    let source_id = source.get("tmdbId").and_then(Value::as_i64);
    let media_type = if source
        .get("mediaType")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("TV"))
    {
        "tv"
    } else {
        "movie"
    };
    let requested_type = if media_type == "tv" {
        "series"
    } else {
        "movie"
    };
    let actual_type = if source_type == "NETWORK" {
        "tv"
    } else {
        media_type
    };
    let mut params = serde_json::Map::from_iter([
        ("api_key".into(), json!(api_key)),
        ("language".into(), json!(language)),
        ("page".into(), json!(page)),
    ]);
    let path = match (source_type, source_id) {
        ("LIST", Some(id)) => format!("3/list/{id}"),
        ("COLLECTION", Some(id)) => {
            params.remove("page");
            format!("3/collection/{id}")
        }
        ("PERSON" | "DIRECTOR", Some(id)) => {
            params.remove("page");
            format!("3/person/{id}/combined_credits")
        }
        _ => {
            params.insert(
                "sort_by".into(),
                source
                    .get("sortBy")
                    .cloned()
                    .unwrap_or_else(|| json!("popularity.desc")),
            );
            if source_type == "COMPANY" {
                if let Some(id) = source_id {
                    params.insert("with_companies".into(), json!(id));
                }
            }
            if source_type == "NETWORK" {
                if let Some(id) = source_id {
                    params.insert("with_networks".into(), json!(id));
                }
            }
            let filters = source.get("filters").and_then(Value::as_object);
            for (input, output) in [
                (
                    "year",
                    if actual_type == "tv" {
                        "first_air_date_year"
                    } else {
                        "year"
                    },
                ),
                ("withGenres", "with_genres"),
                ("watchRegion", "watch_region"),
                ("voteCountGte", "vote_count.gte"),
                ("withKeywords", "with_keywords"),
                ("withNetworks", "with_networks"),
                ("withCompanies", "with_companies"),
                (
                    "releaseDateGte",
                    if actual_type == "tv" {
                        "first_air_date.gte"
                    } else {
                        "primary_release_date.gte"
                    },
                ),
                (
                    "releaseDateLte",
                    if actual_type == "tv" {
                        "first_air_date.lte"
                    } else {
                        "primary_release_date.lte"
                    },
                ),
                ("voteAverageGte", "vote_average.gte"),
                ("voteAverageLte", "vote_average.lte"),
                ("withOriginCountry", "with_origin_country"),
                ("withWatchProviders", "with_watch_providers"),
                ("withOriginalLanguage", "with_original_language"),
            ] {
                if let Some(value) = filters
                    .and_then(|values| values.get(input))
                    .filter(|value| value.is_string() || value.is_number())
                {
                    params.insert(output.into(), value.clone());
                }
            }
            format!("3/discover/{actual_type}")
        }
    };
    serde_json::to_string(&json!({"url": format!("https://api.themoviedb.org/{path}"), "params": params, "headers": {}, "responseKind": "tmdb", "sourceType": source_type, "mediaType": media_type, "requestedType": requested_type, "language": language})).ok()
}

pub(crate) fn remote_collection_response_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let plan = request.get("plan")?;
    let data = request.get("data")?;
    if plan.get("responseKind").and_then(Value::as_str) == Some("trakt") {
        let requested_type = plan
            .get("requestedType")
            .and_then(Value::as_str)
            .unwrap_or("movie");
        let metas = data.as_array()?.iter().filter_map(|item| {
            let value = item.get(if requested_type == "series" { "show" } else { "movie" })?;
            let title = value.get("title").and_then(Value::as_str)?;
            let ids = value.get("ids")?;
            let id = ids.get("imdb").and_then(Value::as_str).map(str::to_string).or_else(|| ids.get("tmdb").and_then(Value::as_i64).map(|id| format!("tmdb:{id}")))?;
            Some(json!({"id": id, "type": requested_type, "name": title, "releaseInfo": value.get("year").and_then(Value::as_i64).map(|year| year.to_string())}))
        }).collect::<Vec<_>>();
        return serde_json::to_string(&metas).ok();
    }
    let source_type = plan
        .get("sourceType")
        .and_then(Value::as_str)
        .unwrap_or("DISCOVER");
    let media_type = plan
        .get("mediaType")
        .and_then(Value::as_str)
        .unwrap_or("movie");
    let language = plan.get("language").and_then(Value::as_str).unwrap_or("en");
    let items = match source_type {
        "COLLECTION" => data
            .get("parts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        "LIST" => data
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        "PERSON" => data
            .get("cast")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|item| {
                        item.get("media_type").and_then(Value::as_str) == Some(media_type)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        "DIRECTOR" => data
            .get("crew")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|item| {
                        item.get("job").and_then(Value::as_str) == Some("Director")
                            && item.get("media_type").and_then(Value::as_str) == Some(media_type)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        _ => data
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    if source_type == "LIST" {
        let movies = items
            .iter()
            .filter(|item| item.get("media_type").and_then(Value::as_str) != Some("tv"))
            .cloned()
            .collect::<Vec<_>>();
        let series = items
            .iter()
            .filter(|item| item.get("media_type").and_then(Value::as_str) == Some("tv"))
            .cloned()
            .collect::<Vec<_>>();
        let mut metas: Vec<Value> =
            serde_json::from_str(&crate::tmdb_plan::tmdb_bulk_metas_to_metas_json(
                &Value::Array(movies).to_string(),
                "movie",
                language,
            )?)
            .ok()?;
        let mut series_metas: Vec<Value> =
            serde_json::from_str(&crate::tmdb_plan::tmdb_bulk_metas_to_metas_json(
                &Value::Array(series).to_string(),
                "series",
                language,
            )?)
            .ok()?;
        metas.append(&mut series_metas);
        return serde_json::to_string(&metas).ok();
    }
    crate::tmdb_plan::tmdb_bulk_metas_to_metas_json(
        &Value::Array(items).to_string(),
        plan.get("requestedType")
            .and_then(Value::as_str)
            .unwrap_or("movie"),
        language,
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TogglePlanRequest {
    item: Value,
    #[serde(default)]
    is_currently_in_watchlist: bool,
    #[serde(default)]
    profile_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalMergeRequest {
    #[serde(default)]
    local_items: Vec<Value>,
    #[serde(default)]
    external_items: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollectionImportRequest {
    #[serde(default)]
    collections: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfflineGroupingRequest {
    #[serde(default)]
    items: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgressMergeRequest {
    existing: Value,
    incoming: Value,
}

pub(crate) fn watchlist_toggle_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<TogglePlanRequest>(request_json).ok()?;
    let is_in_watchlist = request.is_currently_in_watchlist;
    let should_add = !is_in_watchlist;
    let item_id = request
        .item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    serde_json::to_string(&json!({
        "command": if should_add { "add" } else { "remove" },
        "itemId": item_id,
        "optimisticIsInWatchlist": should_add,
        "profileId": request.profile_id
    }))
    .ok()
}

pub(crate) fn library_external_merge_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ExternalMergeRequest>(request_json).ok()?;
    let local_ids: std::collections::HashSet<String> = request
        .local_items
        .iter()
        .filter_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();
    let merged_external: Vec<&Value> = request
        .external_items
        .iter()
        .filter(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !local_ids.contains(id))
        })
        .collect();
    let mut merged: Vec<Value> = request.local_items.clone();
    merged.extend(merged_external.into_iter().cloned());
    serde_json::to_string(&json!({
        "merged": merged,
        "localCount": request.local_items.len(),
        "externalOnlyCount": merged.len() - request.local_items.len()
    }))
    .ok()
}

pub(crate) fn library_collection_import_validation_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<CollectionImportRequest>(request_json).ok()?;
    let mut issues = Vec::<String>::new();
    let mut valid_collections = Vec::<Value>::new();
    for (i, col) in request.collections.iter().enumerate() {
        let id = col.get("id").and_then(Value::as_str).unwrap_or("").trim();
        let title = col
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            issues.push(format!("collection[{}]: missing id", i));
            continue;
        }
        if title.is_empty() {
            issues.push(format!("collection[{}]: missing title", i));
            continue;
        }
        valid_collections.push(col.clone());
    }
    serde_json::to_string(&json!({
        "isValid": issues.is_empty(),
        "validCollections": valid_collections,
        "issues": issues
    }))
    .ok()
}

pub(crate) fn library_offline_grouping_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<OfflineGroupingRequest>(request_json).ok()?;
    let mut ready = Vec::<&Value>::new();
    let mut downloading = Vec::<&Value>::new();
    let mut queued = Vec::<&Value>::new();
    let mut failed = Vec::<&Value>::new();
    for item in &request.items {
        match item.get("status").and_then(Value::as_str).unwrap_or("") {
            "ready" | "complete" => ready.push(item),
            "downloading" | "in_progress" => downloading.push(item),
            "failed" | "error" => failed.push(item),
            _ => queued.push(item),
        }
    }
    serde_json::to_string(&json!({
        "ready": ready,
        "downloading": downloading,
        "queued": queued,
        "failed": failed
    }))
    .ok()
}

pub(crate) fn playback_progress_merge_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ProgressMergeRequest>(request_json).ok()?;
    let existing = &request.existing;
    let incoming = &request.incoming;

    let existing_video_id = existing.get("lastVideoId").and_then(Value::as_str);
    let incoming_video_id = incoming.get("lastVideoId").and_then(Value::as_str);
    let video_changed = incoming_video_id.is_some() && incoming_video_id != existing_video_id;

    let resolve_field = |key: &str| -> Value {
        incoming
            .get(key)
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| existing.get(key).cloned())
            .unwrap_or(Value::Null)
    };

    serde_json::to_string(&json!({
        "lastVideoId": resolve_field("lastVideoId"),
        "timeOffset": incoming.get("timeOffset").cloned().unwrap_or(Value::Null),
        "duration": incoming.get("duration").cloned().unwrap_or(Value::Null),
        "lastStreamIndex": resolve_field("lastStreamIndex"),
        "lastEpisodeName": resolve_field("lastEpisodeName"),
        "lastEpisodeSeason": resolve_field("lastEpisodeSeason"),
        "lastEpisodeNumber": resolve_field("lastEpisodeNumber"),
        "lastEpisodeThumbnail": resolve_field("lastEpisodeThumbnail"),
        "lastStreamUrl": resolve_field("lastStreamUrl"),
        "lastStreamTitle": resolve_field("lastStreamTitle"),
        "continueWatchingPoster": resolve_field("continueWatchingPoster"),
        "continueWatchingBackground": resolve_field("continueWatchingBackground"),
        "lastAudioLanguage": resolve_field("lastAudioLanguage"),
        "lastSubtitleLanguage": resolve_field("lastSubtitleLanguage"),
        "videoChanged": video_changed
    }))
    .ok()
}

fn cleaned_url(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn github_blob_url_regex() -> &'static regex::Regex {
    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        regex::Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/blob/([^/]+)/(.+)$")
            .expect("valid github blob url regex")
    })
}

fn cleaned_artwork_url(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim().trim_matches('\'').trim_matches('"').trim();
    if s.is_empty() {
        return None;
    }
    let with_scheme = if s.starts_with("//") {
        format!("https:{s}")
    } else {
        s.to_string()
    };
    let normalized = if let Some(caps) = github_blob_url_regex().captures(&with_scheme) {
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            &caps[1], &caps[2], &caps[3], &caps[4]
        )
    } else {
        with_scheme
    };
    Some(normalized.replace(' ', "%20"))
}

fn pick_str<'a>(obj: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(Value::String(s)) = obj.get(*k) {
            return Some(s.as_str());
        }
    }
    None
}

fn normalize_shape(value: Option<&str>) -> &'static str {
    match value.map(|s| s.trim().to_uppercase()).as_deref() {
        Some("LANDSCAPE") | Some("WIDE") => "wide",
        Some("SQUARE") => "square",
        _ => "poster",
    }
}

fn export_shape(value: Option<&str>) -> &'static str {
    match value.map(str::to_lowercase).as_deref() {
        Some("wide") | Some("landscape") => "LANDSCAPE",
        Some("square") => "SQUARE",
        _ => "POSTER",
    }
}

// FNV-1a over the title: a wasm-safe, deterministic id suffix for imported
// entries that arrive without one (re-importing the same file is idempotent).
fn stable_suffix(seed: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn merge_object(raw: &serde_json::Map<String, Value>, normalized: Value) -> Value {
    let mut merged = raw.clone();
    if let Some(normalized) = normalized.as_object() {
        for (key, value) in normalized {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

pub(crate) fn import_collections_json(raw_json: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(raw_json).ok()?;
    let arr: Vec<&Value> = match parsed.as_array() {
        Some(a) => a.iter().collect(),
        None => vec![&parsed],
    };

    let collections: Vec<Value> = arr.iter().enumerate().filter_map(|(i, col)| {
        let col = col.as_object()?;
        let title = col.get("title")?.as_str()?.trim().to_string();
        if title.is_empty() { return None; }
        let id = col.get("id").and_then(Value::as_str).filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("imported_{}_{i}", stable_suffix(&title)));

        let raw_folders = col.get("folders").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
        let folders: Vec<Value> = raw_folders.iter().enumerate().filter_map(|(fi, f)| {
            let folder = f.as_object()?;
            let folder_title = folder.get("title")?.as_str()?.trim().to_string();
            if folder_title.is_empty() { return None; }
            let fid = folder.get("id").and_then(Value::as_str).filter(|s| !s.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("folder_{}_{fi}", stable_suffix(&folder_title)));

            let raw_sources = folder.get("catalogSources").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
            let mut sources: Vec<Value> = raw_sources.iter().filter_map(|s| {
                let o = s.as_object()?;
                let catalog_id = o.get("catalogId")?.as_str().filter(|s| !s.is_empty())?;
                Some(json!({
                    "catalogId": catalog_id,
                    "type": o.get("type").and_then(Value::as_str).unwrap_or("movie"),
                    "addonId": o.get("addonId").and_then(Value::as_str),
                    "genre": o.get("genre").and_then(Value::as_str),
                }))
            }).collect();

            if sources.is_empty() {
                if let Some(fallback_id) = folder.get("catalogId").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                    sources.push(json!({ "catalogId": fallback_id, "type": "movie" }));
                }
            }
            let nuvio_sources: Vec<Value> = folder.get("sources")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .filter_map(|source| {
                    let provider = source.get("provider")?.as_str()?.to_ascii_lowercase();
                    match provider.as_str() {
                        "trakt" if source.get("traktListId").and_then(Value::as_i64).is_some() => Some(json!({
                            "provider": "trakt",
                            "title": source.get("title").and_then(Value::as_str),
                            "mediaType": source.get("mediaType").and_then(Value::as_str).unwrap_or("MOVIE"),
                            "traktListId": source.get("traktListId"),
                            "sortBy": source.get("sortBy").and_then(Value::as_str).unwrap_or("rank"),
                        "sortHow": source.get("sortHow").and_then(Value::as_str).unwrap_or("asc"),
                    })),
                        "tmdb" if source.get("tmdbSourceType").and_then(Value::as_str).is_some() => Some(json!({
                            "provider": "tmdb",
                            "title": source.get("title").and_then(Value::as_str),
                            "mediaType": source.get("mediaType").and_then(Value::as_str).unwrap_or("MOVIE"),
                            "tmdbSourceType": source.get("tmdbSourceType"),
                            "tmdbId": source.get("tmdbId"),
                            "sortBy": source.get("sortBy").and_then(Value::as_str),
                            "sortHow": source.get("sortHow").and_then(Value::as_str),
                            "filters": source.get("filters").cloned().unwrap_or(Value::Null),
                        })),
                        _ if source.get("addonId").and_then(Value::as_str).is_some()
                            && source.get("type").and_then(Value::as_str).is_some()
                            && source.get("catalogId").and_then(Value::as_str).is_some() => Some(json!({
                            "provider": "addon",
                            "addonId": source.get("addonId"),
                            "type": source.get("type"),
                            "catalogId": source.get("catalogId"),
                            "genre": source.get("genre").and_then(Value::as_str),
                        })),
                        _ => None,
                    }
                })
                .collect();

            let cover_image_url = cleaned_artwork_url(pick_str(folder, &["coverImageUrl","coverUrl","coverImage","cover","poster","thumbnail","thumb"]));
            let image_url = cleaned_artwork_url(pick_str(folder, &["imageUrl","image","image_url","posterUrl","poster_url"]));
            let effective_cover = cover_image_url.or(image_url);
            let hero_backdrop_url = cleaned_url(pick_str(folder, &["heroBackdropUrl","background","backdrop","backgroundUrl","backdropUrl"]));
            let shape = normalize_shape(folder.get("tileShape").or(folder.get("shape")).and_then(Value::as_str));

            Some(merge_object(folder, json!({
                "id": fid,
                "title": folder_title,
                "catalogTitle": folder.get("catalogTitle").and_then(Value::as_str).unwrap_or(&folder_title),
                "catalogId": sources.first().and_then(|s| s.get("catalogId")).and_then(Value::as_str),
                "genre": folder.get("genre").and_then(Value::as_str),
                "shape": shape,
                "hideTitle": folder.get("hideTitle").and_then(Value::as_bool).unwrap_or(false),
                "focusGifEnabled": folder.get("focusGifEnabled").and_then(Value::as_bool).unwrap_or(true),
                "catalogSources": folder.get("catalogSources").cloned().unwrap_or_else(|| if sources.is_empty() { Value::Null } else { json!(sources) }),
                "sources": folder.get("sources").cloned().unwrap_or_else(|| if nuvio_sources.is_empty() { Value::Null } else { json!(nuvio_sources) }),
                "coverEmoji": folder.get("coverEmoji").and_then(Value::as_str),
                "imageUrl": effective_cover,
                "coverImageUrl": effective_cover,
                "focusGifUrl": cleaned_url(folder.get("focusGifUrl").and_then(Value::as_str)),
                "titleLogoUrl": cleaned_url(folder.get("titleLogoUrl").and_then(Value::as_str)),
                "heroBackdropUrl": hero_backdrop_url,
                "heroVideoUrl": cleaned_url(folder.get("heroVideoUrl").and_then(Value::as_str)),
            })))
        }).collect();

        let first_folder_cover = raw_folders.first()
            .and_then(|f| f.as_object())
            .and_then(|f| cleaned_artwork_url(pick_str(f, &["coverImageUrl","coverUrl","coverImage","cover","poster","thumbnail","thumb"]))
                .or_else(|| cleaned_artwork_url(pick_str(f, &["imageUrl","image","image_url","posterUrl","poster_url"]))));

        Some(merge_object(col, json!({
            "id": id,
            "title": title,
            "backdropImageUrl": cleaned_url(col.get("backdropImageUrl").and_then(Value::as_str)),
            "imageUrl": first_folder_cover,
            "showOnHome": col.get("showOnHome").and_then(Value::as_bool).unwrap_or(true),
            "itemIds": [],
            "folders": folders,
            "showAllTab": col.get("showAllTab").and_then(Value::as_bool).unwrap_or(true),
            "viewMode": col.get("viewMode").and_then(Value::as_str).unwrap_or("FOLLOW_LAYOUT"),
            "pinToTop": col.get("pinToTop").and_then(Value::as_bool).unwrap_or(false),
            "focusGlowEnabled": col.get("focusGlowEnabled").and_then(Value::as_bool).unwrap_or(true),
        })))
    }).collect();

    serde_json::to_string(&collections).ok()
}

const AIR_DATE_COOLDOWN_MS: i64 = 12 * 60 * 60 * 1000;

pub(crate) fn air_date_refresh_candidates_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let now_ms = args.get("nowMs").and_then(Value::as_i64)?;
    let items = args.get("items").and_then(Value::as_array)?;

    let parse_ms = |value: Option<&Value>| {
        value
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
    };

    let mut seen: Vec<&str> = Vec::new();
    let mut due: Vec<Value> = Vec::new();
    for item in items {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        if item.get("type").and_then(Value::as_str) != Some("series") {
            continue;
        }
        let next_air = parse_ms(item.get("nextEpisodeAirDate"));
        let missing_or_past = match next_air {
            Some(ms) => ms <= now_ms,
            None => true,
        };
        if !missing_or_past {
            continue;
        }
        let last_checked = parse_ms(item.get("lastAirDateCheckedAt")).unwrap_or(0);
        if now_ms - last_checked >= AIR_DATE_COOLDOWN_MS {
            due.push(Value::String(id.to_string()));
        }
    }
    Some(Value::Array(due).to_string())
}

pub(crate) fn library_view_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let list = |name: &str| {
        args.get(name)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let watchlist = list("watchlist");
    let watching = list("watching");
    let mut completed = list("completed");
    let mut dropped = list("dropped");
    completed.sort_by(|a, b| status_changed_at(b).cmp(status_changed_at(a)));
    dropped.sort_by(|a, b| status_changed_at(b).cmp(status_changed_at(a)));
    let progress: Vec<Value> = args
        .get("progress")
        .and_then(Value::as_object)
        .map(|values| values.values().cloned().collect())
        .unwrap_or_default();
    let all = unique_items(
        watchlist
            .iter()
            .chain(&watching)
            .chain(&completed)
            .chain(&dropped)
            .chain(&progress),
    );
    let mut airing = unique_items(watching.iter().chain(&watchlist));
    airing.retain(|item| {
        item.get("nextEpisodeAirDate")
            .is_some_and(|value| !value.is_null())
            || item
                .get("newEpisodeReleasedAt")
                .is_some_and(|value| !value.is_null())
            || matches!(
                item.get("continueWatchingBadge").and_then(Value::as_str),
                Some("newEpisode" | "scheduledEpisode")
            )
    });
    airing.sort_by_key(air_time);
    let mut rated = all.clone();
    rated.retain(|item| rating(item) >= 7.5);
    rated.sort_by(|a, b| {
        rating(b)
            .partial_cmp(&rating(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut history = all;
    history.retain(|item| activity_time(item) > 0);
    history.sort_by_key(|item| std::cmp::Reverse(activity_time(item)));
    let tab = args.get("tab").and_then(Value::as_str).unwrap_or("");
    let mut items = match tab {
        "watchlist" => watchlist.clone(),
        "watching" => watching.clone(),
        "completed" => completed.clone(),
        "dropped" => dropped.clone(),
        "airing" => airing.clone(),
        "rated" => rated.clone(),
        "history" => history.clone(),
        _ => Vec::new(),
    };
    let tab_items = items.clone();
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !query.is_empty() {
        items.retain(|item| {
            item.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.to_ascii_lowercase().contains(&query))
        });
    }
    match args
        .get("sortBy")
        .and_then(Value::as_str)
        .unwrap_or("default")
    {
        "title" => items.sort_by(|a, b| name(a).cmp(name(b))),
        "rating" => items.sort_by(|a, b| {
            rating(b)
                .partial_cmp(&rating(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| name(a).cmp(name(b)))
        }),
        _ => {}
    }
    serde_json::to_string(&json!({"completed": completed, "dropped": dropped, "smartLists": {"airing": airing, "rated": rated, "history": history}, "tabItems": tab_items, "items": items})).ok()
}

pub(crate) fn collection_merge_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let existing = args.get("existing")?.as_array()?;
    let incoming = args.get("incoming")?.as_array()?;
    let mut merged = existing.clone();
    let mut ids: std::collections::HashSet<&str> = existing
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .collect();
    merged.extend(
        incoming
            .iter()
            .filter(|item| {
                item.get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| ids.insert(id))
            })
            .cloned(),
    );
    serde_json::to_string(&merged).ok()
}

pub(crate) fn collection_folder_items_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let folder = args.get("folder")?;
    let categories = args.get("categories")?.as_array()?;
    let remote_sources = folder
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|source| {
            matches!(
                source.get("provider").and_then(Value::as_str),
                Some("trakt" | "tmdb")
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let modern: Vec<Value> = folder
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|source| source.get("provider").and_then(Value::as_str) == Some("addon"))
        .cloned()
        .collect();
    let fallback = folder
        .get("catalogSources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let sources = if !modern.is_empty() {
        modern
    } else if !fallback.is_empty() {
        fallback
    } else {
        folder.get("catalogId").or_else(|| folder.get("catalog_id")).and_then(Value::as_str)
            .map(|id| vec![json!({"catalogId": id, "type": folder.get("type").or_else(|| folder.get("catalogType"))})]).unwrap_or_default()
    };
    let mut groups: Vec<Value> = Vec::new();
    for source in sources {
        let catalog_id = source
            .get("catalogId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(category) = categories.iter().find(|category| {
            category.get("id").and_then(Value::as_str) == Some(catalog_id)
                || category.get("catalogId").and_then(Value::as_str) == Some(catalog_id)
        }) else {
            continue;
        };
        let content_type = source.get("type").and_then(Value::as_str).unwrap_or("");
        let genre = source
            .get("genre")
            .or_else(|| folder.get("genre"))
            .and_then(Value::as_str);
        let selected: Vec<Value> = category
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|item| {
                genre.is_none_or(|target| {
                    item.get("genres")
                        .and_then(Value::as_array)
                        .is_some_and(|genres| {
                            genres
                                .iter()
                                .filter_map(Value::as_str)
                                .any(|value| value.eq_ignore_ascii_case(target))
                        })
                })
            })
            .cloned()
            .collect();
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.get("type").and_then(Value::as_str) == Some(content_type))
        {
            group.get_mut("items")?.as_array_mut()?.extend(selected);
        } else {
            groups.push(json!({"type": content_type, "items": selected}));
        }
    }
    let items: Vec<Value> = groups
        .iter()
        .flat_map(|group| {
            group
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .cloned()
        })
        .collect();
    serde_json::to_string(
        &json!({"items": items, "groups": groups, "remoteSources": remote_sources}),
    )
    .ok()
}

fn unique_items<'a>(items: impl Iterator<Item = &'a Value>) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    items
        .filter(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty() && seen.insert(id))
        })
        .cloned()
        .collect()
}

fn name(item: &Value) -> &str {
    item.get("name").and_then(Value::as_str).unwrap_or("")
}
fn rating(item: &Value) -> f64 {
    item.get("imdbRating")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}
fn timestamp(item: &Value, key: &str) -> i64 {
    item.get(key)
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp_millis())
        .unwrap_or(0)
}
fn status_changed_at(item: &Value) -> &str {
    item.get("statusChangedAt")
        .and_then(Value::as_str)
        .unwrap_or("")
}
fn activity_time(item: &Value) -> i64 {
    [
        "savedAt",
        "lastWatchedAt",
        "statusChangedAt",
        "newEpisodeReleasedAt",
        "lastAirDateCheckedAt",
        "updatedAt",
    ]
    .iter()
    .map(|key| timestamp(item, key))
    .find(|value| *value > 0)
    .unwrap_or(0)
}
fn air_time(item: &Value) -> i64 {
    let value = timestamp(item, "nextEpisodeAirDate").max(timestamp(item, "newEpisodeReleasedAt"));
    if value > 0 {
        value
    } else {
        i64::MAX
    }
}

pub(crate) fn air_date_refresh_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let items = args.get("items")?.as_array()?;
    let due_ids: Vec<String> =
        serde_json::from_str(&air_date_refresh_candidates_json(args_json)?).ok()?;
    let due: std::collections::HashSet<&str> = due_ids.iter().map(String::as_str).collect();
    let mut seen = std::collections::HashSet::new();
    let candidates: Vec<&Value> = items
        .iter()
        .filter(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| due.contains(id) && seen.insert(id))
        })
        .collect();
    serde_json::to_string(&candidates).ok()
}

pub(crate) fn apply_air_date_updates_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let updates = args.get("updates")?.as_array()?;
    let apply = |items: &Vec<Value>| {
        items
            .iter()
            .map(|item| {
                let Some(update) = updates
                    .iter()
                    .find(|update| update.get("id") == item.get("id"))
                else {
                    return item.clone();
                };
                let mut merged = item.as_object().cloned().unwrap_or_default();
                for key in ["nextEpisodeAirDate", "lastAirDateCheckedAt"] {
                    merged.insert(
                        key.to_string(),
                        update.get(key).cloned().unwrap_or(Value::Null),
                    );
                }
                Value::Object(merged)
            })
            .collect::<Vec<_>>()
    };
    serde_json::to_string(&json!({
        "watchlist": apply(args.get("watchlist")?.as_array()?),
        "continueWatching": apply(args.get("continueWatching")?.as_array()?),
    }))
    .ok()
}

pub(crate) fn export_collections_json(collections_json: &str) -> Option<String> {
    let collections: Vec<Value> = serde_json::from_str(collections_json).ok()?;
    let data: Vec<Value> = collections
        .iter()
        .filter_map(|collection| {
            let mut collection = collection.as_object()?.clone();
            let folders = collection
                .get("folders")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(|folder| {
                    let mut folder = folder.as_object()?.clone();
                    let tile_shape = folder.get("tileShape").cloned().unwrap_or_else(|| {
                        Value::String(
                            export_shape(folder.get("shape").and_then(Value::as_str)).to_string(),
                        )
                    });
                    folder.insert("tileShape".to_string(), tile_shape);
                    folder
                        .entry("hideTitle".to_string())
                        .or_insert_with(|| Value::Bool(false));
                    folder
                        .entry("focusGifEnabled".to_string())
                        .or_insert_with(|| Value::Bool(true));
                    folder
                        .entry("catalogSources".to_string())
                        .or_insert_with(|| Value::Array(Vec::new()));
                    folder
                        .entry("sources".to_string())
                        .or_insert_with(|| Value::Array(Vec::new()));
                    Some(Value::Object(folder))
                })
                .collect();
            collection.insert("folders".to_string(), Value::Array(folders));
            collection
                .entry("showAllTab".to_string())
                .or_insert_with(|| Value::Bool(true));
            collection
                .entry("viewMode".to_string())
                .or_insert_with(|| Value::String("FOLLOW_LAYOUT".to_string()));
            collection
                .entry("pinToTop".to_string())
                .or_insert_with(|| Value::Bool(false));
            Some(Value::Object(collection))
        })
        .collect();
    serde_json::to_string(&data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn toggle_plan_adds_when_not_in_watchlist() {
        let result: Value = serde_json::from_str(
            &watchlist_toggle_plan_json(
                r#"{"item":{"id":"tt1","type":"movie"},"isCurrentlyInWatchlist":false}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["command"], "add");
        assert_eq!(result["optimisticIsInWatchlist"], true);
    }

    #[test]
    fn toggle_plan_removes_when_in_watchlist() {
        let result: Value = serde_json::from_str(
            &watchlist_toggle_plan_json(
                r#"{"item":{"id":"tt1","type":"movie"},"isCurrentlyInWatchlist":true}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["command"], "remove");
        assert_eq!(result["optimisticIsInWatchlist"], false);
    }

    #[test]
    fn external_merge_deduplicates_preferring_local() {
        let result: Value = serde_json::from_str(
            &library_external_merge_plan_json(
                r#"{"localItems":[{"id":"tt1","source":"local"}],"externalItems":[{"id":"tt1","source":"external"},{"id":"tt2","source":"external"}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let merged = result["merged"].as_array().unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0]["source"], "local");
        assert_eq!(merged[1]["source"], "external");
        assert_eq!(merged[1]["id"], "tt2");
    }

    #[test]
    fn progress_meta_merge_keeps_existing_art_when_incoming_is_blank() {
        let merged = merge_progress_meta_json(
            r#"{"id":"tt1","poster":"","background":"","logo":""}"#,
            r#"{"id":"tt1","poster":"p.jpg","background":"b.jpg","logo":"l.png"}"#,
        );
        let result: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(result["poster"], "p.jpg");
        assert_eq!(result["background"], "b.jpg");
        assert_eq!(result["logo"], "l.png");
    }

    #[test]
    fn collection_import_validation_rejects_missing_id() {
        let result: Value = serde_json::from_str(
            &library_collection_import_validation_json(
                r#"{"collections":[{"title":"My List"},{"id":"c1","title":"Valid"}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["isValid"], false);
        assert_eq!(result["validCollections"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn imports_nuvio_trakt_sources_without_dropping_the_list_id() {
        let result: Value = serde_json::from_str(
            &import_collections_json(r#"[{"id":"streaming","title":"Streaming","folders":[{"id":"netflix","title":"Netflix","sources":[{"provider":"trakt","mediaType":"MOVIE","traktListId":34808160,"sortBy":"rank","sortHow":"asc"},{"provider":"trakt","mediaType":"TV","traktListId":34808679,"sortBy":"rank","sortHow":"asc"}]}]}]"#).unwrap(),
        ).unwrap();
        let sources = result[0]["folders"][0]["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0]["traktListId"], 34808160);
        assert_eq!(sources[1]["mediaType"], "TV");
    }

    #[test]
    fn imports_nuvio_addon_sources_and_folder_artwork() {
        let result: Value = serde_json::from_str(
            &import_collections_json(r#"[{"id":"streaming","title":"Streaming","backdropImageUrl":"https://img.example/backdrop.jpg","folders":[{"id":"catalog","title":"Catalog","tileShape":"wide","heroVideoUrl":"https://video.example/hero.mp4","sources":[{"provider":"addon","addonId":"addon.example","type":"series","catalogId":"top","genre":"Drama"}]}]}]"#).unwrap(),
        ).unwrap();
        let collection = &result[0];
        let folder = &collection["folders"][0];
        assert_eq!(
            collection["backdropImageUrl"],
            "https://img.example/backdrop.jpg"
        );
        assert_eq!(folder["heroVideoUrl"], "https://video.example/hero.mp4");
        assert_eq!(folder["shape"], "wide");
        assert_eq!(folder["sources"][0]["addonId"], "addon.example");
        assert_eq!(folder["sources"][0]["genre"], "Drama");
    }

    #[test]
    fn nuvio_collection_round_trip_preserves_nested_fields() {
        let input = r#"[{"id":"collection","title":"Collection","backdropImageUrl":"https://img.example/backdrop.jpg","futureCollectionField":{"enabled":true},"folders":[{"id":"folder","title":"Folder","tileShape":"wide","heroVideoUrl":"https://video.example/hero.mp4","futureFolderField":[1,2],"sources":[{"provider":"tmdb","tmdbSourceType":"LIST","tmdbId":42,"mediaType":"MOVIE","filters":{"withGenres":"28"},"futureSourceField":"kept"}]}]}]"#;
        let imported = import_collections_json(input).expect("imported");
        let exported = export_collections_json(&imported).expect("exported");
        let result: Value = serde_json::from_str(&exported).expect("json");
        let collection = &result[0];
        let folder = &collection["folders"][0];

        assert_eq!(collection["futureCollectionField"]["enabled"], true);
        assert_eq!(folder["heroVideoUrl"], "https://video.example/hero.mp4");
        assert_eq!(folder["futureFolderField"], json!([1, 2]));
        assert_eq!(folder["sources"][0]["filters"]["withGenres"], "28");
        assert_eq!(folder["sources"][0]["futureSourceField"], "kept");
    }

    #[test]
    fn offline_grouping_partitions_by_status() {
        let result: Value = serde_json::from_str(
            &library_offline_grouping_json(
                r#"{"items":[{"id":"a","status":"ready"},{"id":"b","status":"downloading"},{"id":"c","status":"failed"},{"id":"d"}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["ready"].as_array().unwrap().len(), 1);
        assert_eq!(result["downloading"].as_array().unwrap().len(), 1);
        assert_eq!(result["failed"].as_array().unwrap().len(), 1);
        assert_eq!(result["queued"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn progress_merge_preserves_existing_fields_when_incoming_is_null() {
        let result: Value = serde_json::from_str(
            &playback_progress_merge_plan_json(
                r#"{"existing":{"lastStreamUrl":"http://old","lastVideoId":"v1","timeOffset":1000},"incoming":{"lastVideoId":"v1","timeOffset":2000,"duration":5000}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["timeOffset"], 2000);
        assert_eq!(result["lastStreamUrl"], "http://old");
        assert_eq!(result["videoChanged"], false);
    }

    #[test]
    fn progress_merge_keeps_prior_episode_number_on_video_change_with_incomplete_incoming() {
        let result: Value = serde_json::from_str(
            &playback_progress_merge_plan_json(
                r#"{"existing":{"lastVideoId":"v1","lastEpisodeSeason":1,"lastEpisodeNumber":5,"lastEpisodeName":"Old Name"},"incoming":{"lastVideoId":"v2","timeOffset":0,"duration":5000}}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["videoChanged"], true);
        assert_eq!(result["lastEpisodeSeason"], 1);
        assert_eq!(result["lastEpisodeNumber"], 5);
        assert_eq!(result["lastEpisodeName"], "Old Name");
    }
}

pub(crate) fn library_apply_mark_watched_json(
    lib_json: &str,
    video_ids_json: &str,
) -> Option<String> {
    use crate::library_state::{
        build_continue_watching_from_progress_json, remember_last_watched_episodes_json,
    };

    let updated_lib_str = remember_last_watched_episodes_json(lib_json, video_ids_json);
    let mut lib: serde_json::Map<String, Value> = serde_json::from_str(&updated_lib_str).ok()?;

    let video_ids: Vec<String> = serde_json::from_str(video_ids_json).unwrap_or_default();
    let watched: std::collections::HashSet<&str> = video_ids.iter().map(String::as_str).collect();

    if let Some(ext_cw) = lib
        .get("externalContinueWatching")
        .and_then(Value::as_array)
        .cloned()
    {
        let filtered: Vec<Value> = ext_cw
            .into_iter()
            .filter(|item| {
                let last_vid = item
                    .get("lastVideoId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                last_vid.is_empty() || !watched.contains(last_vid)
            })
            .collect();
        lib.insert("externalContinueWatching".into(), filtered.into());
    }

    let progress_map = lib
        .get("progress")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let cleaned: serde_json::Map<String, Value> = progress_map
        .into_iter()
        .filter(|(_, entry)| {
            let last_vid = entry
                .get("lastVideoId")
                .and_then(Value::as_str)
                .unwrap_or("");
            last_vid.is_empty() || !watched.contains(last_vid)
        })
        .collect();

    let progress_json = serde_json::to_string(&cleaned).unwrap_or_else(|_| "{}".to_string());
    let cw = build_continue_watching_from_progress_json(&progress_json)
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| Value::Array(Vec::new()));

    lib.insert("progress".into(), Value::Object(cleaned));
    lib.insert("continueWatching".into(), cw);

    serde_json::to_string(&Value::Object(lib)).ok()
}

pub(crate) fn merge_progress_meta_json(
    incoming_meta_json: &str,
    existing_meta_json: &str,
) -> String {
    let incoming: Value = serde_json::from_str(incoming_meta_json).unwrap_or(json!({}));
    let existing: Value = serde_json::from_str(existing_meta_json).unwrap_or(json!({}));

    let is_present = |v: &Value| !v.is_null() && v.as_str() != Some("");

    let pick = |key: &str| -> Value {
        incoming
            .get(key)
            .filter(|v| is_present(v))
            .cloned()
            .or_else(|| existing.get(key).cloned())
            .unwrap_or(Value::Null)
    };

    let mut merged = incoming.clone();
    if let Some(obj) = merged.as_object_mut() {
        obj.insert("poster".into(), pick("poster"));
        obj.insert("background".into(), pick("background"));
        obj.insert("logo".into(), pick("logo"));
    }
    serde_json::to_string(&merged).unwrap_or_else(|_| incoming_meta_json.to_string())
}

pub(crate) fn library_command_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let mut library = request.get("library")?.as_object()?.clone();
    let command = request.get("command")?.as_object()?;
    let command_type = command.get("type")?.as_str()?;
    let now_iso = request.get("nowIso").and_then(Value::as_str).unwrap_or("");
    let mut status_mutation = Value::Null;
    let external_action;

    if command_type == "toggleWatchlist" {
        let item = command.get("item")?.clone();
        let id = item.get("id")?.as_str()?;
        let list = library
            .get("watchlist")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let exists = list
            .iter()
            .any(|entry| entry.get("id").and_then(Value::as_str) == Some(id));
        let toggle_plan: Value = watchlist_toggle_plan_json(
            &json!({"item": item.clone(), "isCurrentlyInWatchlist": exists}).to_string(),
        )
        .and_then(|value| serde_json::from_str(&value).ok())?;
        if toggle_plan.get("command").and_then(Value::as_str) == Some("remove") {
            library.insert(
                "watchlist".into(),
                Value::Array(
                    list.into_iter()
                        .filter(|entry| entry.get("id").and_then(Value::as_str) != Some(id))
                        .collect(),
                ),
            );
            status_mutation = json!({"mediaId": id, "status": null, "item": null});
            external_action = json!({"kind": "watchlist", "command": "remove", "item": item});
        } else {
            let mut next = vec![item.clone()];
            next.extend(list);
            library.insert("watchlist".into(), Value::Array(next));
            for field in ["completed", "dropped"] {
                let filtered = library
                    .get(field)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|entry| entry.get("id").and_then(Value::as_str) != Some(id))
                    .collect();
                library.insert(field.into(), Value::Array(filtered));
            }
            status_mutation = json!({"mediaId": id, "status": "watchlist", "item": item});
            external_action =
                json!({"kind": "watchlist", "command": "add", "item": command.get("item")});
        }
    } else if command_type == "toggleLibraryStatus" {
        let field = command.get("list")?.as_str()?;
        if !matches!(field, "completed" | "dropped") {
            return None;
        }
        let mut item = command.get("item")?.clone();
        let id = item.get("id")?.as_str()?.to_string();
        let list = library
            .get(field)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let exists = list
            .iter()
            .any(|entry| entry.get("id").and_then(Value::as_str) == Some(&id));
        if exists {
            library.insert(
                field.into(),
                Value::Array(
                    list.into_iter()
                        .filter(|entry| entry.get("id").and_then(Value::as_str) != Some(&id))
                        .collect(),
                ),
            );
            status_mutation = json!({"mediaId": id, "status": null, "item": null});
            external_action =
                json!({"kind": "status", "list": field, "command": "remove", "item": item});
        } else {
            if let Some(object) = item.as_object_mut() {
                object.insert("statusChangedAt".into(), Value::String(now_iso.to_string()));
            }
            let mut next = vec![item.clone()];
            next.extend(list);
            library.insert(field.into(), Value::Array(next));
            for other in [
                "watchlist",
                if field == "completed" {
                    "dropped"
                } else {
                    "completed"
                },
            ] {
                let filtered = library
                    .get(other)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|entry| entry.get("id").and_then(Value::as_str) != Some(&id))
                    .collect();
                library.insert(other.into(), Value::Array(filtered));
            }
            let mut progress = library
                .get("progress")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            progress.remove(&id);
            let progress_json = Value::Object(progress.clone()).to_string();
            library.insert("progress".into(), Value::Object(progress));
            library.insert(
                "continueWatching".into(),
                crate::library_state::build_continue_watching_from_progress_json(&progress_json)
                    .and_then(|value| serde_json::from_str(&value).ok())
                    .unwrap_or_else(|| json!([])),
            );
            status_mutation = json!({"mediaId": id, "status": field, "item": item});
            external_action = json!({"kind": "status", "list": field, "command": "add", "item": command.get("item")});
        }
    } else if command_type == "markWatched" {
        let ids = command
            .get("videoIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let watched_value = command
            .get("watched")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut watched = library
            .get("watched")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for id in &ids {
            if let Some(id) = id.as_str() {
                watched.insert(id.into(), Value::Bool(watched_value));
            }
        }
        library.insert("watched".into(), Value::Object(watched));
        if watched_value {
            let updated = library_apply_mark_watched_json(
                &Value::Object(library.clone()).to_string(),
                &Value::Array(ids.clone()).to_string(),
            )?;
            library = serde_json::from_str::<Value>(&updated)
                .ok()?
                .as_object()?
                .clone();
        }
        let series_id = command
            .get("seriesId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let meta = command
            .get("meta")
            .or_else(|| command.get("item"))
            .cloned()
            .unwrap_or(Value::Null);
        let content_type = meta.get("type").and_then(Value::as_str).unwrap_or("series");
        let episode_infos = command.get("episodes").and_then(Value::as_array).into_iter().flatten().enumerate().filter_map(|(index, episode)| {
            let season = episode.get("season").and_then(Value::as_i64)?;
            let number = episode.get("episode").or_else(|| episode.get("number")).and_then(Value::as_i64)?;
            Some(json!({
                "contentId": series_id,
                "contentType": content_type,
                "videoId": episode.get("id"),
                "season": season,
                "episode": number,
                "title": episode.get("name").or_else(|| episode.get("title")).cloned().unwrap_or_else(|| ids.get(index).cloned().unwrap_or(Value::Null)),
            }))
        }).collect::<Vec<_>>();
        let progress_info = library.get("progress").and_then(|value| value.get(series_id)).and_then(|progress| {
            Some(json!({
                "contentId": series_id,
                "contentType": progress.get("meta").and_then(|value| value.get("type")).and_then(Value::as_str).unwrap_or(content_type),
                "videoId": progress.get("lastVideoId")?,
                "positionSeconds": progress.get("timeOffset").cloned().unwrap_or_else(|| json!(0)),
                "durationSeconds": progress.get("duration").cloned().unwrap_or_else(|| json!(0)),
                "lastWatched": progress.get("savedAt"),
                "season": progress.get("lastEpisodeSeason"),
                "episode": progress.get("lastEpisodeNumber"),
            }))
        });
        external_action = json!({"kind": "watched", "watched": watched_value, "videoIds": ids, "seriesId": series_id, "episodeInfos": episode_infos, "meta": meta, "progressInfo": progress_info});
    } else {
        return None;
    }
    Some(json!({"library": library, "statusMutation": status_mutation, "externalAction": external_action}).to_string())
}

pub(crate) fn playback_progress_write_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let mut library = request.get("library")?.as_object()?.clone();
    let progress = request.get("progress")?.as_object()?.clone();
    let meta = progress.get("meta")?.as_object()?.clone();
    let content_id = meta.get("id")?.as_str()?.to_string();
    let mut progress_map = library
        .get("progress")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let existing = progress_map
        .get(&content_id)
        .cloned()
        .unwrap_or_else(|| json!({}));
    let merge_request = json!({"existing": existing, "incoming": progress});
    let merge_plan: Value = playback_progress_merge_plan_json(&merge_request.to_string())
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_else(|| json!({}));
    let merged_meta: Value = serde_json::from_str(&merge_progress_meta_json(
        &Value::Object(meta.clone()).to_string(),
        &existing
            .get("meta")
            .cloned()
            .unwrap_or_else(|| json!({}))
            .to_string(),
    ))
    .unwrap_or_else(|_| Value::Object(meta.clone()));
    let mut merged = existing.as_object().cloned().unwrap_or_default();
    for source in [Value::Object(progress.clone()), merge_plan] {
        if let Some(object) = source.as_object() {
            for (key, value) in object {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    merged.insert("meta".into(), merged_meta);
    merged.insert(
        "savedAt".into(),
        request.get("nowIso").cloned().unwrap_or(Value::Null),
    );
    progress_map.insert(content_id.clone(), Value::Object(merged.clone()));
    let progress_json = Value::Object(progress_map.clone()).to_string();
    library.insert("progress".into(), Value::Object(progress_map));
    library.insert(
        "continueWatching".into(),
        crate::library_state::build_continue_watching_from_progress_json(&progress_json)
            .and_then(|value| serde_json::from_str(&value).ok())
            .unwrap_or_else(|| json!([])),
    );
    let duration = merged
        .get("duration")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let external_progress = (duration > 0.0).then(|| {
        json!({
            "contentId": content_id,
            "contentType": meta.get("type").and_then(Value::as_str).unwrap_or("movie"),
            "videoId": merged.get("lastVideoId").and_then(Value::as_str).unwrap_or(&content_id),
            "positionSeconds": merged.get("timeOffset").cloned().unwrap_or_else(|| json!(0)),
            "durationSeconds": duration,
            "lastWatched": request.get("nowMs").cloned().unwrap_or_else(|| json!(0)),
            "season": merged.get("lastEpisodeSeason"),
            "episode": merged.get("lastEpisodeNumber"),
        })
    });
    Some(json!({"library": library, "entry": merged, "contentId": content_id, "externalProgress": external_progress}).to_string())
}
