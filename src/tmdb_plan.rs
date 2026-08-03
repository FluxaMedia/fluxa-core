mod genres_catalog;
mod helpers;
mod meta_conversion;
mod request_plans;

pub(crate) use genres_catalog::{tmdb_builtin_catalog_url, tmdb_builtin_manifest_json};
pub(crate) use helpers::{tmdb_content_type, tmdb_image_url, tmdb_language, tmdb_resolve_id_hint};
pub(crate) use meta_conversion::{
    tmdb_bulk_metas_to_metas_json, tmdb_bulk_videos_to_trailers_json, tmdb_episodes_to_videos_json,
    tmdb_full_meta_to_meta_json, tmdb_meta_to_meta_json, tmdb_pick_logo_json,
    tmdb_video_to_trailer_json,
};
pub(crate) use request_plans::{
    tmdb_builtin_meta_request_plan_json, tmdb_builtin_meta_urls_from_find_json,
    tmdb_builtin_meta_urls_json, tmdb_credits_url_from_find, tmdb_detail_request_plan_json,
    tmdb_detail_request_urls_from_find_json, tmdb_people_images_from_credits,
    tmdb_people_request_plan, tmdb_season_request_url,
};
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

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

    #[test]
    fn tmdb_language_uses_shared_locale_fallbacks() {
        assert_eq!(tmdb_language(""), "en-US");
        assert_eq!(tmdb_language("english_us"), "en-US");
        assert_eq!(tmdb_language("tr_tr"), "tr-TR");
    }
}
