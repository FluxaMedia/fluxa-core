use super::helpers::{imdb_regex, meta_text, push_unique, year_regex, TMDB_ID_PREFIX};
use super::id::{base_content_id, imdb_id, parse_episode_locator};
use super::text::collapse_whitespace;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) fn normalized_billboard_title(value: &str) -> String {
    collapse_whitespace(
        &value
            .to_lowercase()
            .replace('ç', "c")
            .replace('ğ', "g")
            .replace('ı', "i")
            .replace('ö', "o")
            .replace('ş', "s")
            .replace('ü', "u")
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == ' ' {
                    ch
                } else {
                    ' '
                }
            })
            .collect::<String>(),
    )
}

pub(crate) fn content_trakt_key_value(meta: &Value) -> String {
    trakt_identity_key(meta)
}

pub(crate) fn content_merge_keys_value(meta: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    let id = meta_text(meta, "id");
    push_unique(&mut keys, content_trakt_key_value(meta));
    push_unique(&mut keys, id.to_string());
    push_unique(&mut keys, base_content_id(id));
    if let Some(imdb) = imdb_id(id) {
        push_unique(&mut keys, imdb);
    }
    if let Some(key) = title_year_key(meta) {
        push_unique(&mut keys, key);
    }
    keys
}

pub(crate) fn content_watched_keys_value(meta: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    push_unique(&mut keys, content_trakt_key_value(meta));
    push_unique(&mut keys, meta_text(meta, "id").to_string());
    if let Some(key) = title_year_key(meta) {
        push_unique(&mut keys, key);
    }
    keys
}

pub(crate) fn content_trakt_keys_batch(metas_json: &str) -> Option<String> {
    let metas: Vec<Value> = serde_json::from_str(metas_json).ok()?;
    let keys: Vec<String> = metas.iter().map(content_trakt_key_value).collect();
    serde_json::to_string(&keys).ok()
}

pub(crate) fn content_watched_keys_batch(metas_json: &str) -> Option<String> {
    let metas: Vec<Value> = serde_json::from_str(metas_json).ok()?;
    let keys: Vec<Vec<String>> = metas.iter().map(content_watched_keys_value).collect();
    serde_json::to_string(&keys).ok()
}

pub(crate) fn content_keys_json(meta_json: &str, watched: bool) -> Option<String> {
    let meta = serde_json::from_str::<Value>(meta_json).ok()?;
    let keys = if watched {
        content_watched_keys_value(&meta)
    } else {
        content_merge_keys_value(&meta)
    };
    serde_json::to_string(&keys).ok()
}
pub(crate) fn normalized_loose_title(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub(crate) fn title_year_key(meta: &Value) -> Option<String> {
    let title = normalized_loose_title(meta_text(meta, "name"));
    if title.is_empty() {
        return None;
    }
    let year = year_regex()
        .find(meta_text(meta, "releaseInfo"))
        .map(|matched| matched.as_str())
        .unwrap_or("");
    Some(format!("{}:{title}:{year}", meta_text(meta, "type")))
}

pub(crate) fn trakt_identity_key(meta: &Value) -> String {
    let id = meta_text(meta, "id");
    if let Some(value) = imdb_regex().find(id).map(|matched| matched.as_str()) {
        return value.to_string();
    }
    let tmdb = if id.to_ascii_lowercase().starts_with(TMDB_ID_PREFIX) {
        id.strip_prefix(TMDB_ID_PREFIX).unwrap_or(id)
    } else {
        ""
    };
    if !tmdb.is_empty() {
        return format!("tmdb:{tmdb}");
    }
    format!(
        "{}:{}:{}",
        meta_text(meta, "type"),
        normalized_loose_title(meta_text(meta, "name")),
        meta_text(meta, "releaseInfo")
    )
}

pub(crate) fn continue_watching_merge_keys(meta: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    push_unique(&mut keys, trakt_identity_key(meta));
    let id = meta_text(meta, "id");
    let base_id = if parse_episode_locator(id).is_some() {
        parse_episode_locator(id)
            .map(|(base_id, _, _)| {
                if base_id.is_empty() {
                    id.to_string()
                } else {
                    base_id
                }
            })
            .unwrap_or_else(|| id.to_string())
    } else {
        id.to_string()
    };
    push_unique(&mut keys, id.to_string());
    push_unique(&mut keys, base_id);
    if let Some(value) = imdb_regex().find(id).map(|matched| matched.as_str()) {
        push_unique(&mut keys, value.to_string());
    }
    if let Some(value) = title_year_key(meta) {
        push_unique(&mut keys, value);
    }
    keys
}

pub(crate) fn is_trakt_continue_watching_source(meta: &Value) -> bool {
    meta_text(meta, "reason").eq_ignore_ascii_case("Trakt.tv")
}

pub(crate) fn merge_continue_watching_duplicates_json(items_json: &str) -> Option<String> {
    let items = serde_json::from_str::<Vec<Value>>(items_json).ok()?;
    let mut merged: Vec<Value> = Vec::new();
    let mut key_to_index: HashMap<String, usize> = HashMap::new();
    let mut aliases: HashMap<String, String> = HashMap::new();

    for item in items {
        let item_keys = continue_watching_merge_keys(&item);
        let key = item_keys
            .iter()
            .find_map(|item_key| aliases.get(item_key).cloned())
            .or_else(|| item_keys.first().cloned())
            .unwrap_or_default();
        if key.is_empty() {
            merged.push(item);
            continue;
        }

        if let Some(index) = key_to_index.get(&key).copied() {
            let item_is_trakt = is_trakt_continue_watching_source(&item);
            let existing_is_trakt = is_trakt_continue_watching_source(&merged[index]);
            let should_replace = if item_is_trakt {
                true
            } else if existing_is_trakt {
                false
            } else {
                let existing_watched_at = merged[index]
                    .get("lastWatchedAt")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let item_watched_at = item
                    .get("lastWatchedAt")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                item_watched_at >= existing_watched_at
            };
            if should_replace {
                merged[index] = item;
            }
        } else {
            key_to_index.insert(key.clone(), merged.len());
            merged.push(item);
        }
        for item_key in item_keys {
            aliases.insert(item_key, key.clone());
        }
    }

    serde_json::to_string(&merged).ok()
}
