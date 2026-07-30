mod addon_priority;
mod collections;
mod export_push;
mod helpers;
mod profiles;
mod progress_sync;
mod reconciliation;

pub(crate) use addon_priority::{addon_state_json, sort_addons_by_priority_json};
pub(crate) use collections::map_collections_json;
pub(crate) use export_push::{
    collection_request_json, export_push_plan_json, library_item_request_json,
    playback_progress_request_json, watched_items_request_json,
};
pub(crate) use profiles::build_local_profiles_json;
pub(crate) use progress_sync::{
    import_merge_plan_json, library_to_watchlist_json, progress_meta_needs_json,
};
pub(crate) use reconciliation::{addon_reconciliation_plan_json, library_mutation_plan_json};
#[cfg(test)]
mod tests {
    use super::helpers::iso_from_ms;
    use super::*;
    use serde_json::{Value, json};

    fn merge(args: Value) -> Value {
        serde_json::from_str(&import_merge_plan_json(&args.to_string()).unwrap()).unwrap()
    }

    #[test]
    fn watched_episode_removes_its_progress_entry() {
        let result = merge(json!({
            "progress": {},
            "watched": {},
            "library": [],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "series", "video_id": "tt1:1:2",
                "position": 500_000, "duration": 1_000_000,
                "season": 1, "episode": 2, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": [
                { "content_id": "tt1", "content_type": "series", "season": 1, "episode": 3, "watched_at": 1_700_000_100_000i64 },
            ],
        }));
        assert!(result["progress"]["tt1"].is_object());

        let result = merge(json!({
            "progress": {},
            "watched": { "tt1:1:2": true },
            "library": [],
            "addonMetas": {},
            "watchProgress": [],
            "watchHistory": [],
        }));
        assert_eq!(result["watched"]["tt1:1:2"], json!(true));
    }

    #[test]
    fn active_remote_progress_clears_conflicting_watched_flags() {
        let result = merge(json!({
            "progress": {},
            "watched": { "tt1:1:2": true, "tt9": true },
            "library": [],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "series", "video_id": "vid1",
                "position": 500_000, "duration": 1_000_000,
                "season": 1, "episode": 2, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": [],
        }));
        assert!(result["watched"].get("tt1:1:2").is_none());
        assert_eq!(result["watched"]["tt9"], json!(true));
    }

    #[test]
    fn resolved_up_next_saved_at_ignores_history_watched_at() {
        let result = merge(json!({
            "progress": {},
            "watched": {},
            "library": [],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "series", "video_id": "tt1:2:1",
                "position": 0, "duration": 1_000_000,
                "season": 2, "episode": 1, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": [
                { "content_id": "tt1", "content_type": "series", "season": 1, "episode": 9, "watched_at": 1_700_000_500_000i64 },
            ],
        }));
        let entry = &result["progress"]["tt1"];
        assert_eq!(entry["continueWatchingBadge"], json!("upNext"));
        assert_eq!(entry["savedAt"], json!(iso_from_ms(1_700_000_000_000)));
    }

    #[test]
    fn missing_history_keeps_local_watched_untouched() {
        let result = merge(json!({
            "progress": {},
            "watched": { "vid1": true },
            "library": [],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "movie", "video_id": "vid1",
                "position": 500_000, "duration": 1_000_000, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": null,
        }));
        assert_eq!(result["watched"]["vid1"], json!(true));
        assert!(result["progress"].get("tt1").is_none());
    }

    #[test]
    fn mid_progress_entry_is_not_marked_up_next() {
        let result = merge(json!({
            "progress": {},
            "watched": {},
            "library": [{ "content_id": "tt1", "name": "Show", "poster": "p.jpg" }],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "movie", "video_id": "vid1",
                "position": 600_000, "duration": 1_200_000, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": [],
        }));
        let entry = &result["progress"]["tt1"];
        assert!(entry.get("continueWatchingBadge").is_none());
        assert_eq!(entry["timeOffset"], json!(600));
        assert_eq!(entry["meta"]["name"], json!("Show"));
        assert_eq!(entry["meta"]["poster"], json!("p.jpg"));
    }

    #[test]
    fn request_payloads_preserve_episode_progress_and_catalog_sources() {
        let progress: Value = serde_json::from_str(&playback_progress_request_json(&json!({"meta":{"id":"tt1","type":"series"},"videoId":"tt1:2:3","position":400,"duration":1000,"watchedAt":42}).to_string()).unwrap()).unwrap();
        assert_eq!(progress["progress_key"], "tt1_s2e3");
        let collection: Value = serde_json::from_str(&collection_request_json(&json!({"id":"c","title":"C","folders":[{"id":"f","title":"F","catalogSources":[{"addonId":"a","catalogId":"top","type":"movie"}]}]}).to_string()).unwrap()).unwrap();
        assert_eq!(collection["folders"][0]["sources"][0]["provider"], "addon");
    }

    #[test]
    fn addon_reconciliation_preserves_core_addon_state_rules() {
        let plan: Value = serde_json::from_str(&addon_reconciliation_plan_json(&json!({
            "current": [{"id":"old","url":"https://old"},{"id":"keep","url":"https://keep"}],
            "desired": [{"url":"https://keep","enabled":false},{"url":"https://new"}],
            "userId":"user", "profileId":2,
        }).to_string()).unwrap()).unwrap();
        assert_eq!(plan["deleteIds"], json!(["old"]));
        assert_eq!(plan["updates"][0]["payload"]["enabled"], false);
        assert_eq!(plan["creates"][0]["profile_id"], 2);
    }

    #[test]
    fn imported_profile_uses_its_nuvio_avatar_catalog_entry() {
        let result: Value = serde_json::from_str(&build_local_profiles_json(&json!({
            "sessionProfile": {"id":"local","nuvioUserId":"user","nuvioEmail":"user@example.com","nuvioAccessToken":"token"},
            "nuvioProfiles": [{"profile_index":1,"name":"Primary","avatar_id":"avatar-1","avatar_url":null}],
            "avatarCatalog": [{"id":"avatar-1","storage_path":"profiles/avatar-1.png"}],
            "existingProfiles": [{"id":"local","nuvioUserId":"user","nuvioProfileIndex":1}],
        }).to_string()).unwrap()).unwrap();
        assert_eq!(
            result[0]["avatarUrl"],
            json!("https://api.nuvio.tv/storage/v1/object/public/avatars/profiles/avatar-1.png")
        );
    }
}
