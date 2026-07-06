use crate::constants::DEFAULT_LANGUAGE;
use serde_json::{json, Value};

pub(crate) fn tmdb_content_type(content_type: &str) -> &str {
    if content_type == "series" {
        "tv"
    } else {
        "movie"
    }
}

pub(crate) fn tmdb_language(language: &str) -> String {
    match language {
        "" | DEFAULT_LANGUAGE | "english_us" => "en-US".to_string(),
        "tr" | "tr_tr" => "tr-TR".to_string(),
        lang if lang.contains('-') => lang.to_string(),
        lang => format!("{}-{}", lang, lang.to_uppercase()),
    }
}

pub(crate) fn tmdb_image_url(path: Option<&str>, size: &str) -> Option<String> {
    let path = path?.trim();
    if path.is_empty() {
        return None;
    }
    Some(format!("https://image.tmdb.org/t/p/{size}{path}"))
}

pub(crate) fn tmdb_meta_to_meta_json(
    item_json: &str,
    requested_type: &str,
    language: &str,
) -> Option<String> {
    let item: Value = serde_json::from_str(item_json).ok()?;
    let id = item.get("id").and_then(Value::as_i64)?;
    let media_type = item.get("media_type").and_then(Value::as_str).unwrap_or("");
    let has_tv = media_type == "tv" || item.get("first_air_date").is_some();
    let content_type = if requested_type == "series" || has_tv {
        "series"
    } else {
        "movie"
    };
    let name = item
        .get("title")
        .or_else(|| item.get("name"))
        .or_else(|| item.get("original_name"))
        .and_then(Value::as_str)
        .unwrap_or(if language == "tr" {
            "Bilinmeyen"
        } else {
            "Unknown"
        });
    let released = item
        .get("release_date")
        .or_else(|| item.get("first_air_date"))
        .and_then(Value::as_str);
    let poster = tmdb_image_url(item.get("poster_path").and_then(Value::as_str), "w500");
    let background = tmdb_image_url(
        item.get("backdrop_path").and_then(Value::as_str),
        "original",
    );
    serde_json::to_string(&json!({
        "id": format!("tmdb:{id}"),
        "type": content_type,
        "name": name,
        "poster": poster,
        "background": background,
        "releaseInfo": released.map(|r| r.get(..4).unwrap_or(r)),
    }))
    .ok()
}

pub(crate) fn tmdb_video_to_trailer_json(video_json: &str) -> Option<String> {
    let video: Value = serde_json::from_str(video_json).ok()?;
    let site = video
        .get("site")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    if site != "youtube" {
        return None;
    }
    let key = video
        .get("key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let video_type = video
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("Trailer");
    let type_lower = video_type.to_lowercase();
    if !["trailer", "teaser", "clip"].contains(&type_lower.as_str()) {
        return None;
    }
    let title = video
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(video_type);
    serde_json::to_string(&json!({
        "url": format!("https://www.youtube.com/watch?v={key}"),
        "title": title,
        "type": video_type,
    }))
    .ok()
}

pub(crate) fn tmdb_bulk_metas_to_metas_json(
    items_json: &str,
    requested_type: &str,
    language: &str,
) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let metas: Vec<Value> = items
        .iter()
        .filter_map(|item| {
            let s = serde_json::to_string(item).ok()?;
            let meta_json = tmdb_meta_to_meta_json(&s, requested_type, language)?;
            serde_json::from_str(&meta_json).ok()
        })
        .collect();
    serde_json::to_string(&metas).ok()
}

pub(crate) fn tmdb_bulk_videos_to_trailers_json(items_json: &str) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let trailers: Vec<Value> = items
        .iter()
        .filter_map(|item| {
            let s = serde_json::to_string(item).ok()?;
            let json = tmdb_video_to_trailer_json(&s)?;
            serde_json::from_str(&json).ok()
        })
        .collect();
    serde_json::to_string(&trailers).ok()
}

/// Returns (numeric_tmdb_id, already_resolved) — if already_resolved is true
/// the caller can use the id directly without an extra API call.
pub(crate) fn tmdb_resolve_id_hint(content_id: &str) -> (String, bool) {
    let base = content_id.replace("tmdb:", "");
    let base = base.split(':').next().unwrap_or(&base);
    if base.chars().all(|c| c.is_ascii_digit()) && !base.is_empty() {
        return (base.to_string(), true);
    }
    let imdb_part = content_id.split(':').next().unwrap_or(content_id);
    (imdb_part.to_string(), false)
}

fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn tmdb_api_url(path: &str, api_key: &str, language: &str, extra: &[(&str, &str)]) -> String {
    let mut params = format!(
        "api_key={}&language={}",
        encode_query(api_key),
        encode_query(&tmdb_language(language))
    );
    for (key, value) in extra {
        params.push_str(&format!("&{key}={}", encode_query(value)));
    }
    format!("https://api.themoviedb.org/{path}?{params}")
}

fn tmdb_credits_url(content_type: &str, tmdb_id: &str, api_key: &str, language: &str) -> String {
    tmdb_api_url(
        &format!("3/{}/{tmdb_id}/credits", tmdb_content_type(content_type)),
        api_key,
        language,
        &[],
    )
}

fn is_imdb_id(id: &str) -> bool {
    id.len() > 2
        && id[..2].eq_ignore_ascii_case("tt")
        && id[2..].bytes().all(|b| b.is_ascii_digit())
}

pub(crate) fn tmdb_people_request_plan_json(
    meta_json: &str,
    api_key: &str,
    language: &str,
) -> Option<String> {
    let meta: Value = serde_json::from_str(meta_json).ok()?;
    let id = meta.get("id").and_then(Value::as_str).unwrap_or("");
    let content_type = meta.get("type").and_then(Value::as_str).unwrap_or("");

    let (base_id, resolved) = tmdb_resolve_id_hint(id);
    if resolved {
        return Some(
            json!({ "creditsUrl": tmdb_credits_url(content_type, &base_id, api_key, language) })
                .to_string(),
        );
    }

    let imdb_id = id.split(':').next().unwrap_or("");
    if !is_imdb_id(imdb_id) {
        return Some("{}".to_string());
    }
    Some(
        json!({
            "findUrl": tmdb_api_url(
                &format!("3/find/{imdb_id}"),
                api_key,
                language,
                &[("external_source", "imdb_id")],
            ),
        })
        .to_string(),
    )
}

pub(crate) fn tmdb_credits_url_from_find_json(
    find_json: &str,
    meta_json: &str,
    api_key: &str,
    language: &str,
) -> Option<String> {
    let find: Value = serde_json::from_str(find_json).ok()?;
    let meta: Value = serde_json::from_str(meta_json).ok()?;
    let content_type = meta.get("type").and_then(Value::as_str).unwrap_or("");
    let key = if content_type == "series" {
        "tv_results"
    } else {
        "movie_results"
    };
    let result_id = find.get(key)?.as_array()?.first()?.get("id")?;
    let tmdb_id = match result_id {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => return None,
    };
    Some(tmdb_credits_url(content_type, &tmdb_id, api_key, language))
}

fn normalize_person_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn tmdb_people_images_from_credits_json(
    credits_json: &str,
    links_json: &str,
) -> Option<String> {
    let credits: Value = serde_json::from_str(credits_json).ok()?;
    let links: Vec<Value> = serde_json::from_str(links_json).ok()?;

    let mut wanted: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for link in &links {
        if let Some(name) = link.get("name").and_then(Value::as_str) {
            wanted.insert(normalize_person_name(name), name.to_string());
        }
    }

    let empty: Vec<Value> = Vec::new();
    let cast = credits.get("cast").and_then(Value::as_array).unwrap_or(&empty);
    let crew = credits.get("crew").and_then(Value::as_array).unwrap_or(&empty);

    let mut images = serde_json::Map::new();
    for person in cast.iter().chain(crew.iter()) {
        let name = person.get("name").and_then(Value::as_str).unwrap_or("");
        let Some(canonical) = wanted.get(&normalize_person_name(name)) else {
            continue;
        };
        if images.contains_key(canonical) {
            continue;
        }
        if let Some(image) =
            tmdb_image_url(person.get("profile_path").and_then(Value::as_str), "w185")
        {
            images.insert(canonical.clone(), Value::String(image));
        }
    }

    serde_json::to_string(&Value::Object(images)).ok()
}
