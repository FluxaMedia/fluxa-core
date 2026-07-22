use super::*;

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
        let missing_episode_details = [
            "nextEpisodeSeason",
            "nextEpisodeNumber",
            "nextEpisodeTitle",
        ]
        .iter()
        .any(|key| item.get(*key).is_none() || item.get(*key) == Some(&Value::Null));
        if !missing_or_past && !missing_episode_details {
            continue;
        }
        let last_checked = parse_ms(item.get("lastAirDateCheckedAt")).unwrap_or(0);
        if missing_episode_details || now_ms - last_checked >= AIR_DATE_COOLDOWN_MS {
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
                for key in [
                    "nextEpisodeAirDate",
                    "nextEpisodeSeason",
                    "nextEpisodeNumber",
                    "nextEpisodeTitle",
                    "nextEpisodePoster",
                    "lastAirDateCheckedAt",
                ] {
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

