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

pub(crate) fn tmdb_people_request_plan(meta: &Value, api_key: &str, language: &str) -> Value {
    let id = meta.get("id").and_then(Value::as_str).unwrap_or("");
    let content_type = meta.get("type").and_then(Value::as_str).unwrap_or("");

    let (base_id, resolved) = tmdb_resolve_id_hint(id);
    if resolved {
        return json!({ "creditsUrl": tmdb_credits_url(content_type, &base_id, api_key, language) });
    }

    let imdb_id = id.split(':').next().unwrap_or("");
    if !is_imdb_id(imdb_id) {
        return json!({});
    }
    json!({
        "findUrl": tmdb_api_url(
            &format!("3/find/{imdb_id}"),
            api_key,
            language,
            &[("external_source", "imdb_id")],
        ),
    })
}

pub(crate) fn tmdb_credits_url_from_find(
    find: &Value,
    meta: &Value,
    api_key: &str,
    language: &str,
) -> Option<String> {
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

pub(crate) fn tmdb_people_images_from_credits(credits: &Value, links: &[Value]) -> Value {
    let mut wanted: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for link in links {
        if let Some(name) = link.get("name").and_then(Value::as_str) {
            wanted.insert(normalize_person_name(name), name.to_string());
        }
    }

    let empty: Vec<Value> = Vec::new();
    let cast = credits
        .get("cast")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let crew = credits
        .get("crew")
        .and_then(Value::as_array)
        .unwrap_or(&empty);

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

    Value::Object(images)
}

const MOVIE_GENRES: &[(u32, &str)] = &[
    (28, "Action"),
    (12, "Adventure"),
    (16, "Animation"),
    (35, "Comedy"),
    (80, "Crime"),
    (99, "Documentary"),
    (18, "Drama"),
    (10751, "Family"),
    (14, "Fantasy"),
    (36, "History"),
    (27, "Horror"),
    (10402, "Music"),
    (9648, "Mystery"),
    (10749, "Romance"),
    (878, "Science Fiction"),
    (10770, "TV Movie"),
    (53, "Thriller"),
    (10752, "War"),
    (37, "Western"),
];

const TV_GENRES: &[(u32, &str)] = &[
    (10759, "Action & Adventure"),
    (16, "Animation"),
    (35, "Comedy"),
    (80, "Crime"),
    (99, "Documentary"),
    (18, "Drama"),
    (10751, "Family"),
    (10762, "Kids"),
    (9648, "Mystery"),
    (10763, "News"),
    (10764, "Reality"),
    (10765, "Sci-Fi & Fantasy"),
    (10766, "Soap"),
    (10767, "Talk"),
    (10768, "War & Politics"),
    (37, "Western"),
];

fn genre_table(content_type: &str) -> &'static [(u32, &'static str)] {
    if content_type == "series" {
        TV_GENRES
    } else {
        MOVIE_GENRES
    }
}

fn tmdb_genre_id(content_type: &str, name: &str) -> Option<u32> {
    genre_table(content_type)
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
        .map(|(id, _)| *id)
}

fn genre_names(content_type: &str) -> Vec<&'static str> {
    genre_table(content_type).iter().map(|(_, n)| *n).collect()
}

pub(crate) fn tmdb_builtin_manifest_json() -> String {
    let catalog = |content_type: &str, id: &str, name: &str| {
        json!({
            "type": content_type,
            "id": id,
            "name": name,
            "extra": [
                { "name": "search" },
                { "name": "genre", "options": genre_names(content_type) },
                { "name": "skip" },
            ],
        })
    };
    json!({
        "id": "com.fluxa.tmdb-builtin",
        "name": "TMDB",
        "description": "Built-in metadata sourced directly from TMDB",
        "version": "1.0.0",
        "resources": ["catalog", "meta"],
        "types": ["movie", "series"],
        "idPrefixes": ["tt", "tmdb:"],
        "catalogs": [
            catalog("movie", "tmdb.movies", "TMDB Movies"),
            catalog("series", "tmdb.series", "TMDB Series"),
        ],
    })
    .to_string()
}

pub(crate) fn tmdb_builtin_catalog_url(
    content_type: &str,
    extra: &Value,
    api_key: &str,
    language: &str,
) -> String {
    let tmdb_type = tmdb_content_type(content_type);
    let skip = extra
        .get("skip")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0);
    let page = (skip / 20) + 1;
    let page_str = page.to_string();

    if let Some(search) = extra
        .get("search")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return tmdb_api_url(
            &format!("3/search/{tmdb_type}"),
            api_key,
            language,
            &[("query", search), ("page", &page_str)],
        );
    }

    if let Some(genre_name) = extra
        .get("genre")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        if let Some(genre_id) = tmdb_genre_id(content_type, genre_name) {
            let genre_id_str = genre_id.to_string();
            return tmdb_api_url(
                &format!("3/discover/{tmdb_type}"),
                api_key,
                language,
                &[
                    ("with_genres", genre_id_str.as_str()),
                    ("sort_by", "popularity.desc"),
                    ("page", &page_str),
                ],
            );
        }
    }

    tmdb_api_url(
        &format!("3/{tmdb_type}/popular"),
        api_key,
        language,
        &[("page", &page_str)],
    )
}

fn pick_logo(images: &Value, language: &str) -> Option<String> {
    let logos = images.get("logos").and_then(Value::as_array)?;
    let lang = tmdb_language(language);
    let lang_prefix = lang.split('-').next().unwrap_or("en");
    let pick = |want: &str| {
        logos
            .iter()
            .find(|l| l.get("iso_639_1").and_then(Value::as_str) == Some(want))
    };
    let chosen = pick(lang_prefix)
        .or_else(|| pick("en"))
        .or_else(|| logos.first())?;
    tmdb_image_url(chosen.get("file_path").and_then(Value::as_str), "w500")
}

pub(crate) fn tmdb_full_meta_to_meta_json(
    details_json: &str,
    credits_json: &str,
    images_json: &str,
    external_ids_json: &str,
    requested_type: &str,
    language: &str,
) -> Option<String> {
    let details: Value = serde_json::from_str(details_json).ok()?;
    let credits: Value = serde_json::from_str(credits_json).unwrap_or_else(|_| json!({}));
    let images: Value = serde_json::from_str(images_json).unwrap_or_else(|_| json!({}));
    let external_ids: Value = serde_json::from_str(external_ids_json).unwrap_or_else(|_| json!({}));

    let tmdb_id = details.get("id").and_then(Value::as_i64)?;
    let has_tv = details.get("first_air_date").is_some() || details.get("name").is_some();
    let content_type = if requested_type == "series" || has_tv {
        "series"
    } else {
        "movie"
    };

    let imdb_id = external_ids
        .get("imdb_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let id = imdb_id
        .map(str::to_string)
        .unwrap_or_else(|| format!("tmdb:{tmdb_id}"));

    let name = details
        .get("title")
        .or_else(|| details.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Unknown");
    let description = details
        .get("overview")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let released = details
        .get("release_date")
        .or_else(|| details.get("first_air_date"))
        .and_then(Value::as_str);
    let runtime_minutes = details
        .get("runtime")
        .and_then(Value::as_i64)
        .filter(|m| *m > 0)
        .or_else(|| {
            details
                .get("episode_run_time")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_i64)
        });
    let genres: Vec<String> = details
        .get("genres")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|g| g.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let poster = tmdb_image_url(details.get("poster_path").and_then(Value::as_str), "w500");
    let background = tmdb_image_url(
        details.get("backdrop_path").and_then(Value::as_str),
        "original",
    );
    let logo = pick_logo(&images, language);

    let cast: Vec<Value> = credits
        .get("cast")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .take(10)
                .filter_map(|c| {
                    let name = c.get("name").and_then(Value::as_str)?;
                    Some(json!({
                        "name": name,
                        "character": c.get("character").and_then(Value::as_str),
                        "profilePath": tmdb_image_url(c.get("profile_path").and_then(Value::as_str), "w185"),
                    }))
                })
                .collect()
        })
        .unwrap_or_default();
    let director: Vec<String> = credits
        .get("crew")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter(|c| c.get("job").and_then(Value::as_str) == Some("Director"))
                .filter_map(|c| c.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    serde_json::to_string(&json!({
        "id": id,
        "type": content_type,
        "name": name,
        "description": description,
        "poster": poster,
        "background": background,
        "logo": logo,
        "releaseInfo": released.map(|r| r.get(..4).unwrap_or(r)),
        "runtime": runtime_minutes.map(|m| format!("{m} min")),
        "genres": genres,
        "imdbRating": details.get("vote_average").and_then(Value::as_f64),
        "cast": cast,
        "director": director,
    }))
    .ok()
}

pub(crate) fn tmdb_episodes_to_videos_json(season_json: &str, series_id: &str) -> Option<String> {
    let season: Value = serde_json::from_str(season_json).ok()?;
    let episodes = season.get("episodes").and_then(Value::as_array)?;
    let videos: Vec<Value> = episodes
        .iter()
        .filter_map(|ep| {
            let season_num = ep.get("season_number").and_then(Value::as_i64)?;
            let episode_num = ep.get("episode_number").and_then(Value::as_i64)?;
            Some(json!({
                "id": format!("{series_id}:{season_num}:{episode_num}"),
                "title": ep.get("name").and_then(Value::as_str).unwrap_or("Episode"),
                "season": season_num,
                "episode": episode_num,
                "overview": ep.get("overview").and_then(Value::as_str).filter(|s| !s.is_empty()),
                "released": ep.get("air_date").and_then(Value::as_str),
                "thumbnail": tmdb_image_url(ep.get("still_path").and_then(Value::as_str), "w300"),
            }))
        })
        .collect();
    serde_json::to_string(&videos).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_manifest_declares_no_stream_resource() {
        let manifest: Value = serde_json::from_str(&tmdb_builtin_manifest_json()).unwrap();
        let resources: Vec<&str> = manifest["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(resources.contains(&"catalog"));
        assert!(resources.contains(&"meta"));
        assert!(!resources.contains(&"stream"));
    }

    #[test]
    fn full_meta_prefers_imdb_id_when_available() {
        let details =
            json!({"id": 550, "title": "Fight Club", "overview": "...", "vote_average": 8.4})
                .to_string();
        let credits = json!({"cast": [], "crew": []}).to_string();
        let images = json!({"logos": []}).to_string();
        let external_ids = json!({"imdb_id": "tt0137523"}).to_string();
        let result: Value = serde_json::from_str(
            &tmdb_full_meta_to_meta_json(&details, &credits, &images, &external_ids, "movie", "en")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["id"], "tt0137523");
        assert_eq!(result["type"], "movie");
    }

    #[test]
    fn full_meta_falls_back_to_tmdb_id_without_imdb_match() {
        let details = json!({"id": 550, "name": "Some Show"}).to_string();
        let credits = json!({}).to_string();
        let images = json!({}).to_string();
        let external_ids = json!({}).to_string();
        let result: Value = serde_json::from_str(
            &tmdb_full_meta_to_meta_json(
                &details,
                &credits,
                &images,
                &external_ids,
                "series",
                "en",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["id"], "tmdb:550");
        assert_eq!(result["type"], "series");
    }

    #[test]
    fn full_meta_picks_logo_matching_requested_language_first() {
        let details = json!({"id": 1, "title": "X"}).to_string();
        let credits = json!({}).to_string();
        let images = json!({"logos": [
            {"iso_639_1": "en", "file_path": "/en.png"},
            {"iso_639_1": "tr", "file_path": "/tr.png"},
        ]})
        .to_string();
        let external_ids = json!({}).to_string();
        let result: Value = serde_json::from_str(
            &tmdb_full_meta_to_meta_json(&details, &credits, &images, &external_ids, "movie", "tr")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["logo"], "https://image.tmdb.org/t/p/w500/tr.png");
    }

    #[test]
    fn episodes_map_still_path_to_thumbnail() {
        let season = json!({"episodes": [
            {"season_number": 1, "episode_number": 3, "name": "Ep 3", "still_path": "/s3.jpg", "air_date": "2020-01-01"},
        ]})
        .to_string();
        let result: Value =
            serde_json::from_str(&tmdb_episodes_to_videos_json(&season, "tt123").unwrap()).unwrap();
        let video = &result[0];
        assert_eq!(video["id"], "tt123:1:3");
        assert_eq!(video["thumbnail"], "https://image.tmdb.org/t/p/w300/s3.jpg");
    }

    #[test]
    fn catalog_url_maps_genre_name_to_id() {
        let url = tmdb_builtin_catalog_url("movie", &json!({"genre": "Horror"}), "KEY", "en");
        assert!(url.contains("3/discover/movie"));
        assert!(url.contains("with_genres=27"));
    }

    #[test]
    fn catalog_url_maps_skip_to_page() {
        let url = tmdb_builtin_catalog_url("movie", &json!({"skip": 40}), "KEY", "en");
        assert!(url.contains("page=3"));
    }
}
