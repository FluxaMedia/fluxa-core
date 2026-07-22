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

mod collections;

pub(crate) use collections::*;
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
