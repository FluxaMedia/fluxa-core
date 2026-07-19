use crate::addon_protocol::{
    build_resource_url, catalog_supports_extra as manifest_catalog_supports_extra,
    supports_resource,
};
use crate::content_identity::{parse_extra_args_json, stable_feed_part};
use crate::repository_flow::addon_streams_with_provider_json;
use crate::stream_policy::stream_playback_info_json;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceFetchPlanRequest {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    addons: Vec<Value>,
    #[serde(default)]
    transport_url: Option<String>,
    #[serde(default)]
    resource: Option<String>,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    catalog_id: Option<String>,
    #[serde(default)]
    catalog_key: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    request_ids: Vec<String>,
    #[serde(default)]
    extra: Map<String, Value>,
    #[serde(default)]
    extra_raw: String,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    genre: Option<String>,
    #[serde(default)]
    skip: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceParseRequest {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    response: Value,
    #[serde(default)]
    addon_name: Option<String>,
    #[serde(default)]
    season: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackPrepareRequest {
    stream: Value,
    #[serde(default)]
    meta: Option<Value>,
    #[serde(default)]
    episode: Option<Value>,
    #[serde(default)]
    preferred_player: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryLocalStateRequest {
    #[serde(default)]
    library: Value,
    #[serde(default)]
    primary_id: Option<String>,
    #[serde(default)]
    fallback_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreferenceUpdateRequest {
    #[serde(default)]
    existing: Map<String, Value>,
    key: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddonCollectionMutationRequest {
    #[serde(default)]
    existing: Vec<Value>,
    #[serde(default)]
    incoming: Vec<Value>,
    #[serde(default)]
    remove_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailEpisodePlanRequest {
    #[serde(default)]
    episodes: Vec<Value>,
    #[serde(default)]
    selected_season: Option<i64>,
    #[serde(default)]
    selected_episode_id: Option<String>,
    #[serde(default)]
    meta_id: Option<String>,
}

pub(crate) fn resource_fetch_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ResourceFetchPlanRequest>(request_json).ok()?;
    let mut requests = Vec::<Value>::new();

    match request.kind.as_str() {
        "catalogPage" => {
            let transport_url = request.transport_url.as_deref()?;
            let content_type = request.content_type.as_deref()?;
            let catalog_id = request.catalog_id.as_deref()?;
            requests.push(json!({
                "url": build_resource_url(transport_url, "catalog", content_type, catalog_id, extra_json(&request).as_deref()),
                "kind": "catalogPage"
            }));
        }
        "search" => {
            let query = request.query.as_deref().unwrap_or("");
            for addon in &request.addons {
                let Some(transport_url) = addon_transport_url(addon) else {
                    continue;
                };
                for catalog in addon_catalogs(addon) {
                    if !catalog_supports_search(&catalog) {
                        continue;
                    }
                    let Some(content_type) = catalog.get("type").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(id) = catalog.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    requests.push(json!({
                        "url": build_resource_url(transport_url, "catalog", content_type, id, Some(&json!({"search": query}).to_string())),
                        "kind": "search",
                        "addonName": addon_display_name(addon),
                        "catalogId": id,
                        "catalogType": content_type,
                        "categoryId": format!("{}:{}:{}", transport_url, content_type, id),
                        "categoryName": search_category_name(addon, &catalog, content_type)
                    }));
                }
            }
        }
        "discover" => {
            let catalog_key = request.catalog_key.as_deref()?;
            for addon in &request.addons {
                let Some(transport_url) = addon_transport_url(addon) else {
                    continue;
                };
                for catalog in addon_catalogs(addon) {
                    let Some(content_type) = catalog.get("type").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(id) = catalog.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    let key = format!(
                        "discover:{}:{}:{}",
                        stable_feed_part(transport_url),
                        stable_feed_part(content_type),
                        stable_feed_part(id),
                    );
                    if key != catalog_key {
                        continue;
                    }
                    let extra = request
                        .extra
                        .iter()
                        .filter(|(name, _)| catalog_supports_extra(&catalog, name))
                        .map(|(name, value)| (name.clone(), value.clone()))
                        .collect::<Map<_, _>>();
                    let extra = (!extra.is_empty()).then(|| Value::Object(extra).to_string());
                    requests.push(json!({
                        "url": build_resource_url(transport_url, "catalog", content_type, id, extra.as_deref()),
                        "kind": "discover",
                        "catalogKey": key
                    }));
                    break;
                }
                if !requests.is_empty() {
                    break;
                }
            }
        }
        "metaDetail" => {
            let content_type = request.content_type.as_deref()?;
            let id = request.id.as_deref()?;
            for addon in &request.addons {
                if !addon_supports(addon, "meta", content_type, Some(id)) {
                    continue;
                }
                let Some(transport_url) = addon_transport_url(addon) else {
                    continue;
                };
                requests.push(json!({
                    "url": build_resource_url(transport_url, "meta", content_type, id, None),
                    "kind": "metaDetail",
                    "addonName": addon_display_name(addon),
                    "stopOnFirstResult": true
                }));
            }
        }
        "streams" => {
            let content_type = request.content_type.as_deref()?;
            for addon in &request.addons {
                if !addon_supports(addon, "stream", content_type, None) {
                    continue;
                }
                let Some(transport_url) = addon_transport_url(addon) else {
                    continue;
                };
                for id in &request.request_ids {
                    requests.push(json!({
                        "url": build_resource_url(transport_url, "stream", content_type, id, None),
                        "kind": "streams",
                        "addonName": addon_display_name(addon)
                    }));
                }
            }
        }
        "seasonEpisodes" => {
            let series_id = request.id.as_deref()?;
            for addon in &request.addons {
                if !addon_supports(addon, "meta", "series", Some(series_id)) {
                    continue;
                }
                let Some(transport_url) = addon_transport_url(addon) else {
                    continue;
                };
                requests.push(json!({
                    "url": build_resource_url(transport_url, "meta", "series", series_id, None),
                    "kind": "seasonEpisodes",
                    "addonName": addon_display_name(addon),
                    "stopOnFirstResult": true
                }));
            }
        }
        "subtitles" => {
            let content_type = request.content_type.as_deref()?;
            let id = request.id.as_deref()?;
            for addon in &request.addons {
                if !addon_supports(addon, "subtitles", content_type, Some(id)) {
                    continue;
                }
                let Some(transport_url) = addon_transport_url(addon) else {
                    continue;
                };
                requests.push(json!({
                    "url": build_resource_url(transport_url, "subtitles", content_type, id, None),
                    "kind": "subtitles",
                    "addonName": addon_display_name(addon)
                }));
                if !request.extra_raw.trim().is_empty() {
                    requests.push(json!({
                        "url": build_resource_url(
                            transport_url,
                            "subtitles",
                            content_type,
                            id,
                            parse_extra_args_json(&request.extra_raw).as_deref()
                        ),
                        "kind": "subtitles",
                        "addonName": addon_display_name(addon)
                    }));
                }
            }
        }
        _ => {
            let transport_url = request.transport_url.as_deref()?;
            let resource = request.resource.as_deref()?;
            let content_type = request.content_type.as_deref()?;
            let id = request.id.as_deref()?;
            requests.push(json!({
                "url": build_resource_url(transport_url, resource, content_type, id, extra_json(&request).as_deref()),
                "kind": request.kind
            }));
        }
    }

    serde_json::to_string(&json!({ "requests": requests })).ok()
}

/// Wraps `resource_fetch_plan_json` with the execution policy for running its
/// requests: whether to race them (all `stopOnFirstResult`, take the first non-empty
/// result) or fan them out with bounded concurrency, and the retry/timeout budget for
/// stream requests specifically (addon stream endpoints are the flakiest resource kind).
pub(crate) fn resource_fetch_execution_policy_json(request_json: &str) -> Option<String> {
    let plan: Value = serde_json::from_str(&resource_fetch_plan_json(request_json)?).ok()?;
    let requests = plan.get("requests")?.as_array()?.clone();
    let mode = if requests.len() > 1
        && requests
            .iter()
            .all(|r| r.get("stopOnFirstResult").and_then(Value::as_bool) == Some(true))
    {
        "race"
    } else {
        "fanout"
    };
    serde_json::to_string(&json!({
        "requests": requests,
        "mode": mode,
        "concurrency": 12,
        "streamRetry": {
            "maxAttempts": 3,
            "fetchTimeoutMs": 60_000,
            "retryTimeoutMs": 20_000,
        },
    }))
    .ok()
}

pub(crate) fn resource_parse_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<ResourceParseRequest>(request_json).ok()?;
    let value = resource_parse_plan_value(
        &request.kind,
        request.response,
        request.addon_name.as_deref(),
        request.season,
    );
    serde_json::to_string(&value).ok()
}

pub(crate) fn playback_prepare_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<PlaybackPrepareRequest>(request_json).ok()?;
    let info = stream_playback_info_json(&request.stream.to_string())
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    let playable_url = info
        .get("playableUrl")
        .or_else(|| request.stream.get("playableUrl"))
        .or_else(|| request.stream.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_torrent = info
        .get("isTorrentPlaybackUrl")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || playable_url.starts_with("stremio://torrent/")
        || request
            .stream
            .get("infoHash")
            .and_then(Value::as_str)
            .is_some();
    let compatible = info
        .get("isLikelyPlayerCompatible")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let external_url = info
        .get("externalUrl")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mode = if playable_url.is_empty() && external_url.is_some() {
        "external"
    } else if playable_url.is_empty() || !compatible {
        "reject"
    } else if is_torrent {
        "torrent"
    } else {
        "direct"
    };
    serde_json::to_string(&json!({
        "mode": mode,
        "url": if mode == "external" { external_url.clone().unwrap_or_default() } else { playable_url.clone() },
        "isTorrent": is_torrent,
        "rejectReason": if playable_url.is_empty() && external_url.is_none() { "missing_playable_url" } else if !compatible { "incompatible_stream" } else { "" },
        "subtitleExtraArgs": info.get("subtitleExtraArgs").cloned().unwrap_or(Value::Null),
        "title": playback_title(request.meta.as_ref(), request.episode.as_ref(), &request.stream),
        "artwork": playback_artwork(request.meta.as_ref(), request.episode.as_ref()),
        "preferredPlayer": request.preferred_player.unwrap_or_else(|| "mpv".to_string())
    }))
    .ok()
}

pub(crate) fn library_local_state_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<LibraryLocalStateRequest>(request_json).ok()?;
    let id = request
        .primary_id
        .as_deref()
        .or(request.fallback_id.as_deref())
        .unwrap_or("");
    let progress = request
        .library
        .get("progress")
        .and_then(|value| value.get(id))
        .cloned()
        .unwrap_or(Value::Null);
    let is_in_watchlist = request
        .library
        .get("watchlist")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("id").and_then(Value::as_str) == Some(id))
        });
    let watched_video_ids = request
        .library
        .get("watched")
        .and_then(Value::as_object)
        .map(|watched| {
            watched
                .iter()
                .filter(|(key, value)| key.starts_with(id) && value.as_bool().unwrap_or(false))
                .map(|(key, _)| Value::String(key.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::to_string(&json!({
        "progress": progress,
        "isInWatchlist": is_in_watchlist,
        "watchedVideoIds": watched_video_ids
    }))
    .ok()
}

pub(crate) fn preferences_schema_json() -> String {
    json!({
        "keys": [
            "language",
            "startPage",
            "preferredPlayer",
            "streamSourceSelectionMode",
            "streamSourceRegexPattern",
            "preferredAudioLanguage",
            "secondaryAudioLanguage",
            "preferredSubtitleLanguage",
            "secondarySubtitleLanguage",
            "subtitleSize",
            "playbackSpeed",
            "torrentSpeedPreset",
            "torrentCachePreset",
            "downloadSourceSelectionMode",
            "downloadSubtitleLanguage"
        ]
    })
    .to_string()
}

pub(crate) fn apply_preference_update_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<PreferenceUpdateRequest>(request_json).ok()?;
    let mut updated = request.existing;
    let value = normalize_preference_value(&request.key, request.value);
    updated.insert(request.key, value);
    serde_json::to_string(&Value::Object(updated)).ok()
}

pub(crate) fn addon_collection_mutation_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<AddonCollectionMutationRequest>(request_json).ok()?;
    let mut addons = request.existing;
    if let Some(remove_key) = request.remove_key.as_deref() {
        addons.retain(|addon| addon_key(addon) != remove_key);
    }
    for incoming in request.incoming {
        let key = addon_key(&incoming);
        if key.is_empty() {
            continue;
        }
        if let Some(existing) = addons.iter_mut().find(|addon| addon_key(addon) == key) {
            *existing = incoming;
        } else {
            addons.push(incoming);
        }
    }
    serde_json::to_string(&json!({ "addons": addons })).ok()
}

pub(crate) fn detail_episode_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<DetailEpisodePlanRequest>(request_json).ok()?;
    let mut seasons = request
        .episodes
        .iter()
        .filter_map(|episode| episode.get("season").and_then(Value::as_i64).or(Some(1)))
        .collect::<Vec<_>>();
    seasons.sort_unstable();
    seasons.dedup();
    // Search for the target episode across ALL episodes before season filtering,
    // so that a lastVideoId from a later season (e.g. S9 when default would be S1) is found.
    let target_episode = request.selected_episode_id.as_deref().and_then(|id| {
        request
            .episodes
            .iter()
            .find(|ep| ep.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
    });
    let selected_season = target_episode
        .as_ref()
        .and_then(|ep| ep.get("season").and_then(Value::as_i64))
        .or_else(|| {
            request
                .selected_season
                .filter(|season| seasons.contains(season))
        })
        .or_else(|| seasons.first().copied())
        .unwrap_or(1);
    let episodes = request
        .episodes
        .into_iter()
        .filter(|episode| {
            episode.get("season").and_then(Value::as_i64).unwrap_or(1) == selected_season
        })
        .collect::<Vec<_>>();
    let selected_episode = target_episode
        .filter(|ep| ep.get("season").and_then(Value::as_i64).unwrap_or(1) == selected_season)
        .or_else(|| episodes.first().cloned());
    serde_json::to_string(&json!({
        "seasonNumbers": seasons,
        "selectedSeason": selected_season,
        "episodes": episodes,
        "selectedEpisode": selected_episode,
        "streamRequestId": selected_episode
            .as_ref()
            .and_then(|episode| episode.get("id").and_then(Value::as_str))
            .or(request.meta_id.as_deref())
    }))
    .ok()
}

pub(crate) fn season_watched_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let episodes = request.get("episodes")?.as_array()?;
    let watched = request.get("watchedMap")?.as_object()?;
    let seasons = request.get("seasonNumbers")?.as_array()?;
    let mut result = serde_json::Map::new();
    for season in seasons.iter().filter_map(Value::as_i64) {
        let matching: Vec<&Value> = episodes
            .iter()
            .filter(|episode| episode.get("season").and_then(Value::as_i64).unwrap_or(1) == season)
            .collect();
        if !matching.is_empty() {
            result.insert(
                season.to_string(),
                Value::Bool(matching.iter().all(|episode| {
                    episode
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|id| watched.get(id))
                        .and_then(Value::as_bool)
                        == Some(true)
                })),
            );
        }
    }
    serde_json::to_string(&result).ok()
}

pub(crate) fn mark_seasons_action_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let selected: std::collections::HashSet<i64> = request
        .get("seasons")?
        .as_array()?
        .iter()
        .filter_map(Value::as_i64)
        .collect();
    let watched = request
        .get("watched")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let now_ms = request.get("nowMs").and_then(Value::as_i64).unwrap_or(0);
    let episodes: Vec<&Value> = request
        .get("episodes")?
        .as_array()?
        .iter()
        .filter(|episode| {
            selected.contains(&episode.get("season").and_then(Value::as_i64).unwrap_or(1))
        })
        .filter(|episode| {
            !watched
                || episode
                    .get("released")
                    .and_then(Value::as_str)
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .is_none_or(|released| released.timestamp_millis() <= now_ms)
        })
        .collect();
    if episodes.is_empty() {
        return None;
    }
    let meta = request.get("meta")?;
    serde_json::to_string(&json!({
        "type": "markWatchedRequested",
        "seriesId": meta.get("id"),
        "videoIds": episodes.iter().filter_map(|episode| episode.get("id")).collect::<Vec<_>>(),
        "watched": watched,
        "meta": meta,
        "episodes": episodes.iter().map(|episode| json!({
            "id": episode.get("id"),
            "name": episode.get("name").or_else(|| episode.get("title")),
            "season": episode.get("season"),
            "number": episode.get("episode").or_else(|| episode.get("number")),
            "thumbnail": episode.get("thumbnail"),
        })).collect::<Vec<_>>(),
    }))
    .ok()
}

fn extra_json(request: &ResourceFetchPlanRequest) -> Option<String> {
    let mut extra = request.extra.clone();
    if let Some(genre) = request.genre.as_ref().filter(|value| !value.is_empty()) {
        extra.insert("genre".to_string(), Value::String(genre.clone()));
    }
    if let Some(search) = request.query.as_ref().filter(|value| !value.is_empty()) {
        extra.insert("search".to_string(), Value::String(search.clone()));
    }
    if let Some(skip) = request.skip.filter(|value| *value > 0) {
        extra.insert("skip".to_string(), Value::Number(skip.into()));
    }
    (!extra.is_empty()).then(|| Value::Object(extra).to_string())
}

fn addon_transport_url(addon: &Value) -> Option<&str> {
    addon.get("transportUrl").and_then(Value::as_str)
}

fn addon_manifest(addon: &Value) -> Value {
    addon
        .get("manifest")
        .cloned()
        .unwrap_or_else(|| addon.clone())
}

fn addon_catalogs(addon: &Value) -> Vec<Value> {
    addon_manifest(addon)
        .get("catalogs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn addon_supports(addon: &Value, resource: &str, content_type: &str, id: Option<&str>) -> bool {
    let manifest = addon_manifest(addon);
    supports_resource(&manifest.to_string(), resource, Some(content_type), id)
}

fn addon_display_name(addon: &Value) -> String {
    addon
        .get("name")
        .or_else(|| {
            addon
                .get("manifest")
                .and_then(|manifest| manifest.get("name"))
        })
        .and_then(Value::as_str)
        .unwrap_or("Unknown Addon")
        .to_string()
}

fn catalog_supports_extra(catalog: &Value, name: &str) -> bool {
    serde_json::to_string(catalog)
        .ok()
        .is_some_and(|json| manifest_catalog_supports_extra(&json, name))
}

fn catalog_supports_search(catalog: &Value) -> bool {
    catalog_supports_extra(catalog, "search")
}

fn search_category_name(addon: &Value, catalog: &Value, content_type: &str) -> String {
    let addon_name = addon_display_name(addon);
    let catalog_name = catalog
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(match content_type {
            "movie" => "Movies",
            "series" => "Series",
            other => other,
        });
    format!("{addon_name} - {catalog_name}")
}

fn playback_title(meta: Option<&Value>, episode: Option<&Value>, stream: &Value) -> Value {
    let content_title = meta
        .and_then(|value| value.get("name"))
        .or_else(|| stream.get("title"))
        .or_else(|| stream.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Fluxa");
    let season = episode
        .and_then(|value| value.get("season"))
        .and_then(Value::as_i64);
    let episode_number = episode
        .and_then(|value| value.get("episode").or_else(|| value.get("number")))
        .and_then(Value::as_i64);
    let episode_name = episode
        .and_then(|value| value.get("name").or_else(|| value.get("title")))
        .and_then(Value::as_str);
    let episode_line = match (season, episode_number) {
        (Some(season), Some(number)) => {
            let prefix = format!("S{season}:E{number}");
            Some(
                match episode_name.filter(|value| !value.trim().is_empty()) {
                    Some(name) => format!("{prefix} {}", name.trim()),
                    None => prefix,
                },
            )
        }
        _ => None,
    };
    json!({ "contentTitle": content_title, "episodeLine": episode_line })
}

fn playback_artwork(meta: Option<&Value>, episode: Option<&Value>) -> Value {
    let background = meta
        .and_then(|value| {
            first_text(
                value,
                &["background", "backgroundUrl", "backdrop", "backdropUrl"],
            )
        })
        .or_else(|| {
            episode
                .and_then(|value| value.get("thumbnail"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            meta.and_then(|value| value.get("poster"))
                .and_then(Value::as_str)
        });
    let logo =
        meta.and_then(|value| first_text(value, &["logo", "logoUrl", "titleLogo", "titleLogoUrl"]));
    json!({ "background": background, "logo": logo })
}

fn first_text<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
    })
}

fn normalize_preference_value(key: &str, value: Value) -> Value {
    match key {
        "preferredPlayer" => enum_string(value, &["mpv", "exoplayer", "external"], "mpv"),
        "streamSourceSelectionMode" | "downloadSourceSelectionMode" => {
            enum_string(value, &["manual", "first", "best", "regex"], "manual")
        }
        "downloadSubtitleLanguage" => enum_string(
            value,
            &["off", "preferred", "tr", "en", "ja", "es", "fr", "de"],
            "preferred",
        ),
        "torrentSpeedPreset" => enum_string(value, &["default", "fast", "ultra_fast"], "default"),
        "torrentCachePreset" => {
            enum_string(value, &["auto", "2gb", "5gb", "10gb", "unlimited"], "auto")
        }
        "subtitleSize" => enum_string(value, &["50", "75", "100", "125", "150", "200"], "100"),
        _ => value,
    }
}

fn enum_string(value: Value, allowed: &[&str], fallback: &str) -> Value {
    let text = value.as_str().unwrap_or(fallback);
    if allowed.contains(&text) {
        Value::String(text.to_string())
    } else {
        Value::String(fallback.to_string())
    }
}

fn addon_key(addon: &Value) -> String {
    addon
        .get("transportUrl")
        .or_else(|| addon.get("id"))
        .or_else(|| {
            addon
                .get("manifest")
                .and_then(|manifest| manifest.get("id"))
        })
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Maps a request `kind` to the addon resource name used in URLs and responses.
/// `request_resource` and `item_resource` are optional overrides from the request.
pub(crate) fn resource_kind_to_resource(
    kind: &str,
    request_resource: Option<&str>,
    item_resource: Option<&str>,
) -> String {
    let explicit = item_resource
        .filter(|s| !s.trim().is_empty())
        .or_else(|| request_resource.filter(|s| !s.trim().is_empty()));
    if let Some(r) = explicit {
        return r.to_string();
    }
    match kind {
        "catalogPage" | "discover" | "search" => "catalog",
        "metaDetail" | "seasonEpisodes" => "meta",
        "streams" => "stream",
        "subtitles" => "subtitles",
        other if !other.trim().is_empty() => other,
        _ => "catalog",
    }
    .to_string()
}

fn resource_parse_plan_value(
    kind: &str,
    response: Value,
    addon_name: Option<&str>,
    season: Option<i64>,
) -> Value {
    match kind {
        "catalogPage" | "discover" | "search" => {
            json!({ "items": response.get("metas").and_then(Value::as_array).cloned().unwrap_or_default() })
        }
        "metaDetail" => json!({ "meta": response.get("meta").cloned().unwrap_or(Value::Null) }),
        "streams" => {
            let streams = response
                .get("streams")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let normalized = addon_streams_with_provider_json(
                &Value::Array(streams).to_string(),
                addon_name.unwrap_or(""),
            );
            json!({ "streams": serde_json::from_str::<Value>(&normalized).unwrap_or(Value::Array(vec![])) })
        }
        "seasonEpisodes" => {
            let videos = response
                .get("meta")
                .and_then(|meta| meta.get("videos"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|video| {
                    season.is_none() || video.get("season").and_then(Value::as_i64) == season
                })
                .collect::<Vec<_>>();
            json!({ "episodes": videos })
        }
        "subtitles" => {
            json!({ "subtitles": response.get("subtitles").and_then(Value::as_array).cloned().unwrap_or_default() })
        }
        _ => response,
    }
}

pub(crate) fn parse_and_plan_addon_resource_json(
    resource: &str,
    url: &str,
    status_code: i32,
    body: Option<&str>,
    kind: &str,
    addon_name: Option<&str>,
    season: Option<i64>,
) -> String {
    match crate::addon_resource::parse_addon_body(resource, url, status_code, body) {
        crate::addon_resource::ParsedAddonBody::Error(err_json) => err_json,
        crate::addon_resource::ParsedAddonBody::Success { payload, .. } => {
            let wrapped =
                crate::addon_resource::wrap_addon_resource_response_value(resource, payload);
            let value = resource_parse_plan_value(kind, wrapped, addon_name, season);
            json!({ "kind": "success", "value": value }).to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_plan_addon_resource_matches_the_three_call_pipeline_for_discover() {
        let body = r#"{"metas":[{"id":"tt1","name":"A"},{"id":"tt2","name":"B"}]}"#;

        let combined = parse_and_plan_addon_resource_json(
            "catalog",
            "https://addon.example/catalog/movie/top.json",
            200,
            Some(body),
            "discover",
            None,
            None,
        );
        let combined: Value = serde_json::from_str(&combined).expect("combined result");
        assert_eq!(combined["kind"], "success");
        assert_eq!(
            combined["value"]["items"],
            json!([{"id":"tt1","name":"A"},{"id":"tt2","name":"B"}])
        );

        let step1 = crate::addon_resource::parse_addon_resource_result_json(
            "catalog",
            "https://addon.example/catalog/movie/top.json",
            200,
            Some(body),
        );
        let step1: Value = serde_json::from_str(&step1).expect("step1 result");
        let value_json = step1["valueJson"].as_str().expect("valueJson");
        let step2 = crate::addon_resource::wrap_addon_resource_response_value(
            "catalog",
            serde_json::from_str(value_json).unwrap(),
        );
        let step3 =
            resource_parse_plan_json(&json!({ "kind": "discover", "response": step2 }).to_string())
                .expect("step3 result");
        let step3: Value = serde_json::from_str(&step3).expect("step3 value");

        assert_eq!(combined["value"], step3);
    }

    #[test]
    fn parse_and_plan_addon_resource_reports_empty_without_crashing() {
        let combined = parse_and_plan_addon_resource_json(
            "catalog",
            "url",
            200,
            Some(r#"{"metas":[]}"#),
            "discover",
            None,
            None,
        );
        let combined: Value = serde_json::from_str(&combined).expect("combined result");
        assert_eq!(combined["kind"], "empty");
    }

    #[test]
    fn detail_episode_plan_picks_selected_episode_season_over_default() {
        let request = json!({
            "episodes": [
                { "id": "tt1:1:1", "season": 1 },
                { "id": "tt1:1:2", "season": 1 },
                { "id": "tt1:9:1", "season": 9 },
            ],
            "selectedEpisodeId": "tt1:9:1",
            "metaId": "tt1",
        });
        let plan = detail_episode_plan_json(&request.to_string())
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("plan");

        assert_eq!(plan["seasonNumbers"], json!([1, 9]));
        assert_eq!(plan["selectedSeason"], 9);
        assert_eq!(plan["episodes"].as_array().unwrap().len(), 1);
        assert_eq!(plan["selectedEpisode"]["id"], "tt1:9:1");
        assert_eq!(plan["streamRequestId"], "tt1:9:1");
    }

    #[test]
    fn detail_episode_plan_falls_back_to_first_season_and_meta_id() {
        let request = json!({
            "episodes": [
                { "id": "tt1:2:1", "season": 2 },
                { "id": "tt1:3:1", "season": 3 },
            ],
            "metaId": "tt1",
        });
        let plan = detail_episode_plan_json(&request.to_string())
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("plan");

        assert_eq!(plan["selectedSeason"], 2);
        assert_eq!(plan["selectedEpisode"]["id"], "tt1:2:1");
        // No selectedEpisodeId in the request, so streamRequestId falls back to the
        // first episode of the default season, not metaId.
        assert_eq!(plan["streamRequestId"], "tt1:2:1");
    }

    #[test]
    fn resource_fetch_plan_builds_catalog_page_url_with_genre_extra() {
        let request = json!({
            "kind": "catalogPage",
            "transportUrl": "https://addon.example/manifest.json",
            "contentType": "movie",
            "catalogId": "top",
            "genre": "action",
        });
        let plan = resource_fetch_plan_json(&request.to_string())
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("plan");
        let requests = plan["requests"].as_array().unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["kind"], "catalogPage");
        assert!(requests[0]["url"]
            .as_str()
            .unwrap()
            .contains("genre=action"));
    }

    #[test]
    fn resource_fetch_plan_search_only_targets_catalogs_supporting_search() {
        let request = json!({
            "kind": "search",
            "query": "batman",
            "addons": [{
                "transportUrl": "https://addon.example/manifest.json",
                "name": "Addon One",
                "manifest": {
                    "catalogs": [
                        { "id": "top", "type": "movie", "name": "Top Movies", "extraSupported": ["search"] },
                        { "id": "noSearch", "type": "movie", "name": "No Search" },
                    ],
                },
            }],
        });
        let plan = resource_fetch_plan_json(&request.to_string())
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("plan");
        let requests = plan["requests"].as_array().unwrap();

        assert_eq!(
            requests.len(),
            1,
            "catalog without search support must be excluded"
        );
        assert_eq!(requests[0]["catalogId"], "top");
        assert_eq!(requests[0]["categoryName"], "Addon One - Top Movies");
        assert!(requests[0]["url"]
            .as_str()
            .unwrap()
            .contains("search=batman"));
    }
}
