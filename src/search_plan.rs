use crate::{addon_protocol, content_identity};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

// Discover aggregates results from every installed addon's catalogs — with enough
// addons installed, that's thousands of items in one IPC payload. Cap it after
// dedup/sort so a single discover fetch can't balloon into multi-megabyte responses.
const DISCOVER_MAX_ITEMS: usize = 400;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchGroupingRequest {
    #[serde(default)]
    results: Vec<Value>,
    #[serde(default)]
    query: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoverSortRequest {
    #[serde(default)]
    items: Vec<Value>,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    ascending: bool,
    #[serde(default)]
    content_type_filter: Option<String>,
    #[serde(default)]
    genre_filter: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySortRequest {
    #[serde(default)]
    items: Vec<Value>,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    ascending: bool,
    #[serde(default)]
    type_filter: Option<String>,
    #[serde(default)]
    status_filter: Option<String>,
}

fn manifest_value(addon: &Value) -> Option<&Value> {
    addon.get("manifest").or(Some(addon))
}

fn addon_transport_url(addon: &Value) -> &str {
    addon
        .get("transportUrl")
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn addon_manifest_name(addon: &Value) -> String {
    let manifest = manifest_value(addon).unwrap_or(addon);
    manifest
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| manifest.get("id").and_then(Value::as_str))
        .unwrap_or("Metadata")
        .to_string()
}

fn title_label(value: &str) -> String {
    let label = value
        .split(['_', '-', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if label.is_empty() {
        value.to_string()
    } else {
        label
    }
}

fn metadata_feed_home_title(label: &str) -> String {
    let parts = label
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match parts.len() {
        0 => label.to_string(),
        1 => parts[0].to_string(),
        2 => parts[1].to_string(),
        _ => parts[1..].join(" "),
    }
}

fn discover_catalog_label(raw_name: Option<&str>, id: &str) -> String {
    let fallback = title_label(id);
    let base = raw_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&fallback);
    let mut label = base
        .split(['-', ':', '|', '/'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    for word in [
        "cinemeta", "movie", "movies", "film", "films", "series", "shows", "tv",
    ] {
        label = label
            .split_whitespace()
            .filter(|part| !part.eq_ignore_ascii_case(word))
            .collect::<Vec<_>>()
            .join(" ");
    }
    if label.trim().is_empty() {
        fallback
    } else {
        label
    }
}

fn catalog_extras(catalog: &Value) -> Vec<Value> {
    catalog
        .get("extra")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|extra| {
            let name = extra.get("name").and_then(Value::as_str)?.trim();
            if name.is_empty() {
                return None;
            }
            let options = extra
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|option| option.as_str().map(str::trim).map(str::to_string))
                .filter(|option| !option.is_empty())
                .collect::<Vec<_>>();
            (!options.is_empty()).then(|| {
                json!({
                    "name": name,
                    "options": options,
                    "isRequired": catalog_requires_extra(catalog, name)
                })
            })
        })
        .collect()
}

fn manifest_supports_catalog(manifest: &Value) -> bool {
    serde_json::to_string(manifest)
        .ok()
        .is_some_and(|json| addon_protocol::supports_resource(&json, "catalog", None, None))
}

fn catalog_has_required_extra_except(catalog: &Value, allowed: &[&str]) -> bool {
    let allowed_json =
        serde_json::to_string(&allowed.iter().map(|s| s.to_string()).collect::<Vec<_>>())
            .unwrap_or_else(|_| "[]".to_string());
    serde_json::to_string(catalog)
        .ok()
        .is_some_and(|json| addon_protocol::catalog_has_required_extra_except(&json, &allowed_json))
}

fn catalog_requires_extra(catalog: &Value, extra_name: &str) -> bool {
    serde_json::to_string(catalog)
        .ok()
        .is_some_and(|json| addon_protocol::catalog_requires_extra(&json, extra_name))
}

pub(crate) fn build_metadata_feed_options_json(addons_json: &str) -> Option<String> {
    let addons = serde_json::from_str::<Vec<Value>>(addons_json).ok()?;
    let mut feeds = Vec::new();
    for addon in addons {
        let Some(manifest) = manifest_value(&addon) else {
            continue;
        };
        if !manifest_supports_catalog(manifest) {
            continue;
        }
        let addon_name = addon_manifest_name(&addon);
        let transport_url = addon_transport_url(&addon);
        let source_key = manifest
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(transport_url);
        for catalog in manifest
            .get("catalogs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(type_value) = catalog
                .get("type")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Some(id) = catalog
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            else {
                continue;
            };
            if catalog_has_required_extra_except(catalog, &[]) {
                continue;
            }
            let name = catalog
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| title_label(id));
            let key = format!(
                "addon:{}:{}:{}",
                content_identity::stable_feed_part(source_key),
                content_identity::stable_feed_part(type_value),
                content_identity::stable_feed_part(id)
            );
            let label = format!("{addon_name} - {name}");
            feeds.push(json!({
                "key": key,
                "label": label,
                "homeTitle": metadata_feed_home_title(&label),
                "transportUrl": transport_url,
                "type": type_value,
                "id": id,
                "genre": Value::Null
            }));
        }
    }
    serde_json::to_string(&feeds).ok()
}

pub(crate) fn discover_catalog_options_json(
    addons_json: &str,
    selected_type: &str,
) -> Option<String> {
    let addons = serde_json::from_str::<Vec<Value>>(addons_json).ok()?;
    let lower_selected_type = selected_type.to_lowercase();
    let normalized_type = content_identity::normalize_content_type(&lower_selected_type)
        .map(str::to_string)
        .unwrap_or(lower_selected_type);
    let mut options = Vec::new();
    for addon in addons {
        let Some(manifest) = manifest_value(&addon) else {
            continue;
        };
        if !manifest_supports_catalog(manifest) {
            continue;
        }
        let transport_url = addon_transport_url(&addon);
        for catalog in manifest
            .get("catalogs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(type_value) = catalog
                .get("type")
                .and_then(Value::as_str)
                .and_then(content_identity::normalize_content_type)
            else {
                continue;
            };
            if normalized_type != "all" && normalized_type != type_value {
                continue;
            }
            let Some(id) = catalog
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            else {
                continue;
            };
            let extras = catalog_extras(catalog);
            let supported_extra_names = extras
                .iter()
                .filter_map(|extra| extra.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>();
            if catalog_has_required_extra_except(catalog, &supported_extra_names) {
                continue;
            }
            let catalog_label =
                discover_catalog_label(catalog.get("name").and_then(Value::as_str), id);
            options.push(json!({
                "key": format!(
                    "discover:{}:{}:{}",
                    content_identity::stable_feed_part(transport_url),
                    content_identity::stable_feed_part(type_value),
                    content_identity::stable_feed_part(id)
                ),
                "label": format!("{}: {}", addon_manifest_name(&addon), catalog_label),
                "transportUrl": transport_url,
                "type": type_value,
                "id": id,
                "extras": extras
            }));
        }
    }
    serde_json::to_string(&options).ok()
}

pub(crate) fn search_result_grouping_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<SearchGroupingRequest>(request_json).ok()?;
    let mut movies: Vec<&Value> = Vec::new();
    let mut series: Vec<&Value> = Vec::new();
    let mut other: Vec<&Value> = Vec::new();
    for item in &request.results {
        match item.get("type").and_then(Value::as_str).unwrap_or("") {
            "movie" => movies.push(item),
            "series" | "anime" => series.push(item),
            _ => other.push(item),
        }
    }
    let mut groups = Vec::new();
    if !movies.is_empty() {
        groups.push(json!({ "type": "movie", "items": movies }));
    }
    if !series.is_empty() {
        groups.push(json!({ "type": "series", "items": series }));
    }
    if !other.is_empty() {
        groups.push(json!({ "type": "other", "items": other }));
    }
    serde_json::to_string(&json!({
        "groups": groups,
        "totalCount": request.results.len(),
        "query": request.query
    }))
    .ok()
}

pub(crate) fn discover_sort_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<DiscoverSortRequest>(request_json).ok()?;
    let content_type = request.content_type_filter.as_deref().unwrap_or("");
    let genre = request.genre_filter.as_deref().unwrap_or("").to_lowercase();
    let sort_by = match request.sort_by.as_deref().unwrap_or("default") {
        "top" => "rating",
        "newest" => "year",
        other => other,
    };

    let mut filtered: Vec<&Value> = request
        .items
        .iter()
        .filter(|item| {
            let type_ok = content_type.is_empty()
                || content_type == "anime"
                || item
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t == content_type);
            let genre_ok = genre.is_empty()
                || item
                    .get("genres")
                    .and_then(Value::as_array)
                    .is_some_and(|g| {
                        g.iter()
                            .any(|gv| gv.as_str().is_some_and(|s| s.to_lowercase() == genre))
                    });
            type_ok && genre_ok
        })
        .collect();

    let mut seen_ids: HashSet<&str> = HashSet::with_capacity(filtered.len());
    filtered.retain(|item| match item.get("id").and_then(Value::as_str) {
        Some(id) => seen_ids.insert(id),
        None => true,
    });

    match sort_by {
        "year" => {
            filtered.sort_by(|a, b| {
                let ya = a
                    .get("releaseInfo")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0);
                let yb = b
                    .get("releaseInfo")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0);
                if request.ascending {
                    ya.cmp(&yb)
                } else {
                    yb.cmp(&ya)
                }
            });
        }
        "rating" => {
            filtered.sort_by(|a, b| {
                let ra = a.get("imdbRating").and_then(Value::as_f64).unwrap_or(0.0);
                let rb = b.get("imdbRating").and_then(Value::as_f64).unwrap_or(0.0);
                if request.ascending {
                    ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
                }
            });
        }
        "name" => {
            filtered.sort_by(|a, b| {
                let na = a.get("name").and_then(Value::as_str).unwrap_or("");
                let nb = b.get("name").and_then(Value::as_str).unwrap_or("");
                if request.ascending {
                    na.cmp(nb)
                } else {
                    nb.cmp(na)
                }
            });
        }
        _ => {}
    }

    let total_count = filtered.len();
    filtered.truncate(DISCOVER_MAX_ITEMS);

    serde_json::to_string(&json!({
        "items": filtered,
        "sortBy": sort_by,
        "ascending": request.ascending,
        "totalCount": total_count
    }))
    .ok()
}

pub(crate) fn library_sort_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<LibrarySortRequest>(request_json).ok()?;
    let type_filter = request.type_filter.as_deref().unwrap_or("").to_lowercase();
    let status_filter = request
        .status_filter
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let sort_by = request.sort_by.as_deref().unwrap_or("added");

    let mut filtered: Vec<&Value> = request
        .items
        .iter()
        .filter(|item| {
            let type_ok = type_filter.is_empty()
                || item
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.to_lowercase() == type_filter);
            let status_ok = status_filter.is_empty()
                || item
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.to_lowercase() == status_filter);
            type_ok && status_ok
        })
        .collect();

    match sort_by {
        "name" => {
            filtered.sort_by(|a, b| {
                let na = a.get("name").and_then(Value::as_str).unwrap_or("");
                let nb = b.get("name").and_then(Value::as_str).unwrap_or("");
                if request.ascending {
                    na.cmp(nb)
                } else {
                    nb.cmp(na)
                }
            });
        }
        "year" => {
            filtered.sort_by(|a, b| {
                let ya = a
                    .get("releaseInfo")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0);
                let yb = b
                    .get("releaseInfo")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0);
                if request.ascending {
                    ya.cmp(&yb)
                } else {
                    yb.cmp(&ya)
                }
            });
        }
        "progress" => {
            filtered.sort_by(|a, b| {
                let pa = a.get("timeOffset").and_then(Value::as_i64).unwrap_or(0);
                let pb = b.get("timeOffset").and_then(Value::as_i64).unwrap_or(0);
                if request.ascending {
                    pa.cmp(&pb)
                } else {
                    pb.cmp(&pa)
                }
            });
        }
        _ => {}
    }

    serde_json::to_string(&json!({
        "items": filtered,
        "sortBy": sort_by,
        "totalCount": filtered.len()
    }))
    .ok()
}

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

/// Given a catalog source `{catalogId, type, addonId?}` and an array of addon
/// descriptors, returns the first matching `transportUrl`, or `null`.
pub(crate) fn resolve_transport_url_json(source_json: &str, addons_json: &str) -> Option<String> {
    let source: Value = serde_json::from_str(source_json).ok()?;
    let addons: Vec<Value> = serde_json::from_str(addons_json).ok()?;

    let src_addon_id = source
        .get("addonId")
        .and_then(Value::as_str)
        .map(str::to_lowercase);
    let src_catalog_id = source.get("catalogId").and_then(Value::as_str)?;
    let normalize_type = |v: &str| -> String {
        match v.trim().to_lowercase().as_str() {
            "movies" => "movie".to_string(),
            "series" | "tv" | "show" | "shows" => "series".to_string(),
            other => other.to_string(),
        }
    };
    let src_type = source
        .get("type")
        .and_then(Value::as_str)
        .map(normalize_type);

    for addon in &addons {
        let manifest = addon.get("manifest")?;
        let addon_id = manifest
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        let t_url = addon
            .get("transportUrl")
            .and_then(Value::as_str)
            .unwrap_or("");
        if let Some(ref wanted_addon_id) = src_addon_id {
            if !(addon_id == *wanted_addon_id
                || t_url.to_lowercase().contains(wanted_addon_id.as_str()))
            {
                continue;
            }
        }
        let catalogs = manifest
            .get("catalogs")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let matches = catalogs.iter().any(|cat| {
            cat.get("id").and_then(Value::as_str) == Some(src_catalog_id)
                && src_type.as_deref().is_none_or(|st| {
                    cat.get("type").and_then(Value::as_str).map(&normalize_type)
                        == Some(st.to_string())
                })
        });
        if matches {
            return serde_json::to_string(t_url).ok();
        }
    }
    None
}

/// Resolves the effective genre for a metadata feed option by inspecting the
/// corresponding catalog's `extra` array for a `genre` field with a default or
/// first required value.
pub(crate) fn resolve_feed_option_genre_json(
    feed_option_json: &str,
    addons_json: &str,
) -> Option<String> {
    let option: Value = serde_json::from_str(feed_option_json).ok()?;
    let addons: Vec<Value> = serde_json::from_str(addons_json).ok()?;

    // If genre is already set on the option, return it.
    if let Some(genre) = option
        .get("genre")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
    {
        return serde_json::to_string(genre).ok();
    }

    let transport_url = option.get("transportUrl").and_then(Value::as_str)?;
    let opt_type = option.get("type").and_then(Value::as_str)?;
    let opt_id = option.get("id").and_then(Value::as_str)?;

    let addon = addons
        .iter()
        .find(|a| a.get("transportUrl").and_then(Value::as_str) == Some(transport_url))?;
    let catalogs = addon
        .get("manifest")
        .and_then(|m| m.get("catalogs"))
        .and_then(Value::as_array)?;
    let catalog = catalogs.iter().find(|cat| {
        cat.get("type").and_then(Value::as_str) == Some(opt_type)
            && cat.get("id").and_then(Value::as_str) == Some(opt_id)
    })?;

    let extras = catalog.get("extra").and_then(Value::as_array)?;
    let genre_extra = extras
        .iter()
        .find(|e| e.get("name").and_then(Value::as_str) == Some("genre"))?;

    let default_genre = genre_extra
        .get("default")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    let is_required = genre_extra
        .get("isRequired")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let first_option = genre_extra
        .get("options")
        .and_then(Value::as_array)
        .and_then(|opts| opts.first())
        .and_then(Value::as_str);

    let resolved = default_genre.or(if is_required { first_option } else { None })?;
    serde_json::to_string(resolved).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn search_grouping_separates_movies_series_and_other() {
        let result: Value = serde_json::from_str(
            &search_result_grouping_json(
                r#"{"query":"breaking","results":[
                    {"id":"tt1","type":"series","name":"Breaking Bad"},
                    {"id":"tt2","type":"movie","name":"Breaking"},
                    {"id":"tt3","type":"other","name":"Another"}
                ]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let groups = result["groups"].as_array().unwrap();
        assert_eq!(groups[0]["type"], "movie");
        assert_eq!(groups[1]["type"], "series");
        assert_eq!(groups[2]["type"], "other");
    }

    #[test]
    fn discover_sort_filters_by_content_type() {
        let result: Value = serde_json::from_str(
            &discover_sort_plan_json(
                r#"{"contentTypeFilter":"movie","items":[{"id":"tt1","type":"movie"},{"id":"tt2","type":"series"}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn library_sort_filters_by_type() {
        let result: Value = serde_json::from_str(
            &library_sort_plan_json(
                r#"{"typeFilter":"movie","items":[{"id":"tt1","type":"movie"},{"id":"tt2","type":"series"}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn metadata_feed_options_preserve_custom_stremio_catalog_types() {
        let result: Value = serde_json::from_str(
            &build_metadata_feed_options_json(
                r#"[{"transportUrl":"https://aio.example/stremio/u/manifest.json","manifest":{"id":"aiometadata","name":"AIOMetadata","resources":["catalog"],"catalogs":[{"type":"anime.movie","id":"mal.top","name":"MAL Top"},{"type":"Trakt","id":"trakt.upnext","name":"Up Next"}]}}]"#,
            )
            .unwrap(),
        )
        .unwrap();
        let feeds = result.as_array().unwrap();
        assert_eq!(feeds.len(), 2);
        assert_eq!(feeds[0]["type"], "anime.movie");
        assert_eq!(feeds[1]["type"], "Trakt");
    }

    #[test]
    fn series_lookup_id_extracts_imdb_id() {
        assert_eq!(detail_series_lookup_id("tt1234567:1:2"), "tt1234567");
        assert_eq!(detail_series_lookup_id("tt9999999"), "tt9999999");
    }

    #[test]
    fn series_lookup_id_strips_episode_parts_for_non_imdb() {
        assert_eq!(detail_series_lookup_id("kitsu:777:1:2"), "kitsu:777");
        assert_eq!(detail_series_lookup_id("tmdb:12345:1:2"), "tmdb:12345");
    }

    #[test]
    fn season_load_plan_uses_saved_season_when_valid() {
        let result: Value = serde_json::from_str(
            &detail_season_load_plan_json(r#"{"savedVideoId":"tt1:3:2","seasonsCount":5}"#)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["firstSeasonToLoad"], 3);
    }

    #[test]
    fn season_load_plan_defaults_to_season_1_when_no_saved() {
        let result: Value =
            serde_json::from_str(&detail_season_load_plan_json(r#"{"seasonsCount":5}"#).unwrap())
                .unwrap();
        assert_eq!(result["firstSeasonToLoad"], 1);
    }
}
