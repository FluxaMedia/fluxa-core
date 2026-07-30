use super::helpers::{tmdb_image_url, tmdb_language};
use serde_json::{Value, json};

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
