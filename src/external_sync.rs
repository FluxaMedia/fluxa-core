mod calendar;
mod mal;
mod merge;
mod plan;
mod simkl;
mod stremio;
mod trakt;

pub(crate) use calendar::provider_calendar_items_json;
pub(crate) use mal::mal_list_update_json;
pub(crate) use merge::{
    merge_continue_watching_lists_json, merge_external_watched_json,
    merge_external_watchlist_json, merge_watched_timestamped_json,
    merge_watchlist_timestamped_json, ranked_winner, saved_at_ms,
};
pub(crate) use plan::{
    external_provider_action_plan_json, external_sync_refresh_retry_action,
    external_sync_response_action, import_apply_plan_json, promote_external_progress_plan_json,
    provider_pagination_plan_json, push_plan_json,
};
pub(crate) use simkl::{simkl_history_request_json, simkl_watchlist_request_json};
pub(crate) use stremio::{
    stremio_library_mutation_plan_json, stremio_watched_to_ids_json,
    stremio_watchlist_to_items_json,
};
pub(crate) use trakt::{
    trakt_bearer, trakt_comments_request_json, trakt_content_id_from_ids_json,
    trakt_episode_locator_json, trakt_has_client, trakt_history_request_json,
    trakt_id_from_source, trakt_ids_from_content_id_json, trakt_oauth_error_code,
    trakt_playback_delete_ids_json, trakt_playback_items_to_library_json, trakt_playback_url,
    trakt_scrobble_media_id, trakt_scrobble_url, trakt_show_id_from_episode_id,
    trakt_sync_item_to_meta_json, trakt_token_expires_at, trakt_watched_to_ids_json,
    trakt_watchlist_to_items_json,
};

mod provider_mappers;

pub(crate) use provider_mappers::{
    replace_external_continue_watching_json, simkl_lookup_id_for_type,
    simkl_mark_watched_body_json, simkl_match_episode_json, simkl_merge_delta_json,
    simkl_recommendation_candidates_json, simkl_recommendation_to_meta_json,
    simkl_resource_sync_plan_json, simkl_watched_to_ids_json, simkl_watching_to_items_json,
    simkl_watchlist_body_json, simkl_watchlist_to_items_json, trakt_activity_diff_json,
    trakt_mark_watched_body_json, trakt_playback_items_dedup_json,
    trakt_related_items_to_metas_json, trakt_related_lookup_slug,
    trakt_watched_shows_to_items_json,
};
mod anilist;

pub(crate) use anilist::{
    anilist_entries_to_sync, anilist_media_list_status,
    anilist_save_media_list_entry_variables_json, anilist_search_best_match_json,
    extract_anilist_id_from_links, merge_library_items_by_id,
};
#[cfg(test)]
mod tests {
    use super::*;
    use super::trakt::trakt_playback_item_to_library;
    use crate::player_scrobble;
    use serde_json::{Value, json};

    #[test]
    fn trakt_calendar_items_include_episode_identity() {
        let request = json!({
            "provider": "trakt",
            "shows": [{
                "first_aired": "2026-07-27T03:00:00Z",
                "show": {
                    "title": "Rick and Morty",
                    "ids": {"imdb": "tt2861424"},
                    "images": {"poster": ["walter-r2.trakt.tv/images/shows/poster.webp"]}
                },
                "episode": {
                    "season": 9,
                    "number": 10,
                    "title": "Episode Title",
                    "images": {"screenshot": ["walter-r2.trakt.tv/images/episodes/screenshot.webp"]}
                }
            }],
            "movies": []
        });
        let result: Value =
            serde_json::from_str(&provider_calendar_items_json(&request.to_string()).unwrap())
                .unwrap();
        assert_eq!(result[0]["seasonNumber"], 9);
        assert_eq!(result[0]["episodeNumber"], 10);
        assert_eq!(result[0]["metaType"], "series");
        assert_eq!(
            result[0]["episodePoster"],
            "https://walter-r2.trakt.tv/images/episodes/screenshot.webp"
        );
    }

    #[test]
    fn simkl_calendar_items_accept_number_field_variants() {
        let request = json!({
            "provider": "simkl",
            "shows": [{
                "date": "2026-07-27T03:00:00Z",
                "show": {
                    "title": "Rick and Morty",
                    "ids": {"imdb": "tt2861424"}
                },
                "episode": {
                    "season_number": 9,
                    "episode_number": 10,
                    "title": "Episode Title"
                }
            }],
            "movies": []
        });
        let result: Value =
            serde_json::from_str(&provider_calendar_items_json(&request.to_string()).unwrap())
                .unwrap();
        assert_eq!(result[0]["seasonNumber"], 9);
        assert_eq!(result[0]["episodeNumber"], 10);
    }

    #[test]
    fn simkl_calendar_items_accept_v2_cdn_payloads() {
        let request = json!({
            "provider": "simkl",
            "shows": {
                "calendar": [{"simkl_id": 3437, "date": "2026-07-27T04:00:00Z", "episode": {"season": 15, "episode": 10, "title": "Propane Recall"}}],
                "metadata": {"3437": {"title": "King of the Hill", "poster": "https://example.test/poster.jpg", "ids": {"imdb": "tt0118375"}}}
            },
            "movies": {"calendar": [], "metadata": {}},
            "allowedContentIds": ["tt0118375"]
        });
        let result: Value =
            serde_json::from_str(&provider_calendar_items_json(&request.to_string()).unwrap())
                .unwrap();
        assert_eq!(result[0]["contentId"], "tt0118375");
        assert_eq!(result[0]["seasonNumber"], 15);
        assert_eq!(result[0]["episodeNumber"], 10);
        assert_eq!(result[0]["poster"], "https://example.test/poster.jpg");
    }

    #[test]
    fn replace_external_continue_watching_sorts_by_saved_at_descending() {
        let items = json!([
            {"id": "tt1", "reason": "Nuvio", "timeOffset": 100, "duration": 1000, "savedAt": "2026-07-16T16:18:15Z"},
            {"id": "tt2", "reason": "Nuvio", "timeOffset": 100, "duration": 1000, "savedAt": "2026-07-18T22:15:23Z"},
            {"id": "tt3", "reason": "Nuvio", "timeOffset": 100, "duration": 1000, "savedAt": "2026-07-17T19:11:51Z"},
        ]);
        let result = replace_external_continue_watching_json(
            "[]",
            Some("Nuvio"),
            &items.to_string(),
            None,
            None,
            None,
        );
        let parsed: Vec<Value> = serde_json::from_str(&result).unwrap();
        let ids: Vec<&str> = parsed.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["tt2", "tt3", "tt1"]);
    }

    #[test]
    fn merge_continue_watching_sorts_the_combined_result_by_saved_at_descending() {
        let local = json!([
            {"id": "tt1", "savedAt": "2026-07-16T16:18:15Z"},
            {"id": "tt3", "savedAt": "2026-07-17T19:11:51Z"},
        ]);
        let external = json!([
            {"id": "tt2", "savedAt": "2026-07-18T22:15:23Z"},
        ]);
        let result = merge_continue_watching_lists_json(
            &local.to_string(),
            &external.to_string(),
            "{}",
            None,
            None,
        )
        .unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&result).unwrap();
        let ids: Vec<&str> = parsed.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["tt2", "tt3", "tt1"]);
    }

    #[test]
    fn extracts_anilist_id_from_link_url() {
        let meta = json!({"links": [{"name": "AniList", "category": "other", "url": "https://anilist.co/anime/1535"}]});
        assert_eq!(extract_anilist_id_from_links(&meta), Some(1535));
    }

    #[test]
    fn extract_anilist_id_returns_none_without_matching_link() {
        let meta = json!({"links": [{"name": "IMDb", "category": "other", "url": "https://imdb.com/title/tt1"}]});
        assert_eq!(extract_anilist_id_from_links(&meta), None);
    }

    #[test]
    fn extract_anilist_id_skips_malformed_link_and_uses_next_match() {
        let meta = json!({"links": [
            {"name": "Bad", "category": "other", "url": "https://anilist.co/anime/"},
            {"name": "Good", "category": "other", "url": "https://anilist.co/anime/1535"},
        ]});
        assert_eq!(extract_anilist_id_from_links(&meta), Some(1535));
    }

    #[test]
    fn search_match_prefers_exact_title_within_year_tolerance() {
        let args = json!({
            "meta": {"name": "Attack on Titan", "year": 2013},
            "candidates": [
                {"id": 1, "seasonYear": 2020, "title": {"romaji": "Something Else"}},
                {"id": 2, "seasonYear": 2013, "title": {"english": "Attack on Titan"}},
            ],
        });
        let result = anilist_search_best_match_json(&args.to_string()).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["anilistId"], 2);
        assert_eq!(parsed["confidence"], "title-year");
    }

    #[test]
    fn search_match_falls_back_to_year_only_when_no_title_matches() {
        let args = json!({
            "meta": {"name": "Some Show", "year": 2013},
            "candidates": [
                {"id": 3, "seasonYear": 2013, "title": {"romaji": "Different Name"}},
            ],
        });
        let result = anilist_search_best_match_json(&args.to_string()).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["anilistId"], 3);
    }

    #[test]
    fn media_list_status_completes_when_progress_reaches_total() {
        assert_eq!(anilist_media_list_status(12, 12), "COMPLETED");
        assert_eq!(anilist_media_list_status(12, 5), "CURRENT");
        assert_eq!(anilist_media_list_status(0, 5), "CURRENT");
    }

    #[test]
    fn anilist_current_entries_build_progress_and_watched_keys() {
        let entries = r#"[
            {"status":"CURRENT","progress":3,"updatedAt":1700000000,"media":{"id":5,"title":{"romaji":"Show"},"episodes":12}},
            {"status":"PLANNING","media":{"id":6,"title":{"english":"Other"}}}
        ]"#;
        let plan = anilist_entries_to_sync(
            serde_json::from_str::<Vec<Value>>(entries)
                .unwrap()
                .as_slice(),
            0,
            None,
            false,
        );
        assert_eq!(plan["watching"][0]["lastVideoId"], "anilist:5:1:3");
        assert_eq!(plan["watched"]["anilist:5:1:2"], Value::Bool(true));
        assert_eq!(
            plan["progress"]["anilist:5"]["savedAt"],
            "2023-11-14T22:13:20.000Z"
        );
        assert_eq!(plan["watchlist"][0]["inWatchlist"], Value::Bool(true));
        assert_eq!(plan["watchlist"][0]["name"], "Other");
        assert_eq!(plan["watchlist"][0]["updatedAtMs"], json!(0));
    }

    #[test]
    fn anilist_planning_entry_carries_updated_at_ms_when_present() {
        let entries = r#"[
            {"status":"PLANNING","updatedAt":1700000000,"media":{"id":9,"title":{"romaji":"Planned"}}}
        ]"#;
        let plan = anilist_entries_to_sync(
            serde_json::from_str::<Vec<Value>>(entries)
                .unwrap()
                .as_slice(),
            0,
            None,
            false,
        );
        assert_eq!(plan["watchlist"][0]["updatedAtMs"], json!(1700000000000i64));
    }

    #[test]
    fn merge_by_id_overlays_incoming_fields_onto_local_items() {
        let local: Vec<Value> = serde_json::from_str(
            r#"[{"id":"a","name":"Old","poster":"p"},{"id":"b","name":"Keep"}]"#,
        )
        .unwrap();
        let incoming: Vec<Value> =
            serde_json::from_str(r#"[{"id":"a","name":"New"},{"id":"c","name":"Added"}]"#).unwrap();
        let merged = merge_library_items_by_id(&local, &incoming);
        assert_eq!(merged[0]["name"], "New");
        assert_eq!(merged[0]["poster"], "p");
        assert_eq!(merged[1]["name"], "Keep");
        assert_eq!(merged[2]["name"], "Added");
    }

    #[test]
    fn stremio_episode_entries_become_watched_keys_not_watchlist_items() {
        let items = r#"[
            {"_id":"tt1","name":"A Movie","type":"movie","poster":"p.jpg","state":{"flaggedWatched":1}},
            {"_id":"tt2","name":"A Show","type":"series","state":{"flaggedWatched":0}},
            {"_id":"tt2:1:3","name":"A Show","type":"series","state":{"flaggedWatched":1}},
            {"_id":"tt3","name":"Removed","type":"movie","removed":true,"state":null}
        ]"#;
        let watchlist: Vec<Value> =
            serde_json::from_str(&stremio_watchlist_to_items_json(items).unwrap()).unwrap();
        let ids: Vec<&str> = watchlist
            .iter()
            .filter_map(|i| i.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["tt1", "tt2"]);

        let watched: Value =
            serde_json::from_str(&stremio_watched_to_ids_json(items).unwrap()).unwrap();
        assert_eq!(watched.get("tt1"), Some(&Value::Bool(true)));
        assert_eq!(watched.get("tt2:1:3"), Some(&Value::Bool(true)));
        assert_eq!(watched.get("tt2"), None);
    }

    #[test]
    fn trakt_ids_support_stremio_episode_ids() {
        assert_eq!(
            trakt_ids_from_content_id_json("tt1234567:1:2")
                .and_then(|json| serde_json::from_str::<Value>(&json).ok())
                .and_then(|ids| ids.get("imdb").and_then(Value::as_str).map(str::to_owned))
                .as_deref(),
            Some("tt1234567")
        );
        assert_eq!(
            trakt_ids_from_content_id_json("tmdb:42:1:2")
                .and_then(|json| serde_json::from_str::<Value>(&json).ok())
                .and_then(|ids| ids.get("tmdb").and_then(Value::as_i64)),
            Some(42)
        );
    }

    #[test]
    fn trakt_token_expiry_stays_in_epoch_seconds() {
        assert_eq!(trakt_token_expires_at(1_700_000_000, 3_600), 1_700_003_300);
    }

    #[test]
    fn trakt_urls_accept_only_supported_routes() {
        assert_eq!(
            trakt_scrobble_url("pause").as_deref(),
            Some("https://api.trakt.tv/scrobble/pause")
        );
        assert_eq!(trakt_scrobble_url("delete"), None);
        assert_eq!(
            trakt_playback_url(Some("series")).as_deref(),
            Some("https://api.trakt.tv/sync/playback/episodes")
        );
        assert_eq!(trakt_playback_url(Some("unknown")), None);
    }

    #[test]
    fn external_sync_wire_fixtures_preserve_provider_contracts() {
        let trakt_input: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/trakt_scrobble_plan_input.json"
        ))
        .unwrap();
        let trakt_expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/trakt_scrobble_plan_expected.json"
        ))
        .unwrap();
        let trakt_actual: Value = serde_json::from_str(
            &player_scrobble::trakt_scrobble_plan_json(
                &trakt_input["ids"].to_string(),
                trakt_input["isEpisode"].as_bool().unwrap(),
                None,
                None,
                trakt_input["timePosSec"].as_f64().unwrap(),
                trakt_input["durationSec"].as_f64().unwrap(),
                None,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(trakt_actual, trakt_expected);

        let simkl_input =
            include_str!("../tests/fixtures/external_sync/simkl_mark_watched_input.json");
        let simkl_expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/simkl_mark_watched_expected.json"
        ))
        .unwrap();
        let simkl_actual: Value =
            serde_json::from_str(&simkl_mark_watched_body_json(simkl_input).unwrap()).unwrap();
        assert_eq!(simkl_actual, simkl_expected);

        let trakt_playback_expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/trakt_playback_expected.json"
        ))
        .unwrap();
        let trakt_playback_actual: Value = serde_json::from_str(
            &trakt_playback_items_to_library_json(include_str!(
                "../tests/fixtures/external_sync/trakt_playback_response.json"
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(trakt_playback_actual, trakt_playback_expected);

        let simkl_response: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/simkl_watched_response.json"
        ))
        .unwrap();
        let simkl_watched_expected: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/external_sync/simkl_watched_expected.json"
        ))
        .unwrap();
        let simkl_watched_actual: Value = serde_json::from_str(
            &simkl_watched_to_ids_json(
                &simkl_response["shows"].to_string(),
                &simkl_response["movies"].to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(simkl_watched_actual, simkl_watched_expected);
    }

    #[test]
    fn trakt_playback_tmdb_show_keeps_a_resolvable_episode_id() {
        let item = json!({
            "progress": 50.0,
            "paused_at": "2026-07-21T00:00:00.000Z",
            "show": {"title": "Show", "runtime": 45, "ids": {"tmdb": 42}},
            "episode": {"season": 1, "number": 2, "runtime": 45}
        });
        let result = trakt_playback_item_to_library(&item).expect("playback item");
        assert_eq!(result["id"], "tmdb:42");
        assert_eq!(result["lastVideoId"], "tmdb:42:1:2");
    }

    #[test]
    fn trakt_watched_shows_create_continue_watching_items() {
        let watched = json!([{
            "last_watched_at": "2026-07-21T00:00:00.000Z",
            "completed": 4,
            "show": {"title": "Example", "aired_episodes": 8, "ids": {"imdb": "tt42"}},
            "seasons": [{"number": 1, "episodes": [
                {"number": 4, "last_watched_at": "2026-07-21T00:00:00.000Z"}
            ]}]
        }]);
        let items: Value = serde_json::from_str(
            &trakt_watched_shows_to_items_json(&watched.to_string()).expect("items"),
        )
        .unwrap();
        assert_eq!(items[0]["id"], "tt42");
        assert_eq!(items[0]["lastVideoId"], "tt42:1:4");
        assert_eq!(items[0]["timeOffset"], 1);
    }

    #[test]
    fn trakt_watched_shows_without_episode_timestamps_use_latest_episode() {
        let watched = json!([{
            "last_watched_at": "2026-07-21T00:00:00.000Z",
            "completed": 3,
            "show": {"title": "Example", "aired_episodes": 12, "ids": {"imdb": "tt42"}},
            "seasons": [
                {"number": 1, "episodes": [{"number": 3}]},
                {"number": 9, "episodes": [{"number": 3}]}
            ]
        }]);
        let items: Value = serde_json::from_str(
            &trakt_watched_shows_to_items_json(&watched.to_string()).expect("items"),
        )
        .unwrap();
        assert_eq!(items[0]["lastVideoId"], "tt42:9:3");
    }

    #[test]
    fn trakt_watched_shows_use_furthest_episode_over_newer_earlier_rewatch() {
        let watched = json!([{
            "last_watched_at": "2026-07-21T00:00:00.000Z",
            "completed": 3,
            "show": {"title": "Example", "aired_episodes": 12, "ids": {"imdb": "tt42"}},
            "seasons": [
                {"number": 1, "episodes": [{"number": 2, "last_watched_at": "2026-07-21T00:00:00.000Z"}]},
                {"number": 9, "episodes": [{"number": 2, "last_watched_at": "2026-07-01T00:00:00.000Z"}]}
            ]
        }]);
        let items: Value = serde_json::from_str(
            &trakt_watched_shows_to_items_json(&watched.to_string()).expect("items"),
        )
        .unwrap();
        assert_eq!(items[0]["lastVideoId"], "tt42:9:2");
    }

    #[test]
    fn trakt_playback_dedup_keeps_the_furthest_watched_episode() {
        let items = json!([
            {"id": "tt42", "lastEpisodeSeason": 1, "lastEpisodeNumber": 1, "savedAt": "2026-07-22T00:00:00.000Z"},
            {"id": "tt42", "lastEpisodeSeason": 1, "lastEpisodeNumber": 2, "savedAt": "2026-07-21T00:00:00.000Z", "continueWatchingBadge": "upNext"}
        ]);
        let result: Value = serde_json::from_str(
            &trakt_playback_items_dedup_json(&items.to_string()).expect("deduped items"),
        )
        .unwrap();
        assert_eq!(result[0]["lastEpisodeNumber"], 2);
    }

    #[test]
    fn simkl_watching_items_are_kept_for_continue_watching() {
        let items = simkl_watching_to_items_json(
            r#"{"shows":[{"show":{"title":"Example","ids":{"imdb":"tt42"}},"last_watched":"S01E02","last_watched_at":"2026-07-21T00:00:00.000Z","seasons":[{"number":1,"episodes":[{"number":2}]}]}]}"#,
            "[]",
        )
        .expect("items");
        let replaced: Value = serde_json::from_str(&replace_external_continue_watching_json(
            "[]",
            Some("simkl"),
            &items,
            None,
            None,
            None,
        ))
        .unwrap();
        assert_eq!(replaced[0]["id"], "tt42");
        assert_eq!(replaced[0]["lastVideoId"], "tt42:1:2");
        assert_eq!(replaced[0]["lastEpisodeSeason"], 1);
        assert_eq!(replaced[0]["lastEpisodeNumber"], 2);
    }

    #[test]
    fn external_list_mappers_skip_invalid_records_and_keep_valid_ones() {
        let trakt: Vec<Value> = serde_json::from_str(
            &trakt_watchlist_to_items_json(
                r#"[{"movie":{"title":"Valid","ids":{"tmdb":7}}},{"movie":{"title":"Invalid","ids":{}}}]"#,
                "[]",
            )
            .expect("trakt items"),
        )
        .unwrap();
        assert_eq!(trakt.len(), 1);
        assert_eq!(trakt[0]["id"], "tmdb:7");

        let simkl: Vec<Value> = serde_json::from_str(
            &simkl_watchlist_to_items_json(
                r#"[{"show":{"title":"Valid","ids":{"tmdb":7}}},{"show":{"title":"Invalid","ids":{}}}]"#,
                "[]",
            )
            .expect("simkl items"),
        )
        .unwrap();
        assert_eq!(simkl.len(), 1);
        assert_eq!(simkl[0]["id"], "tmdb:7");
    }

    #[test]
    fn watched_mappers_retain_tmdb_only_records() {
        let trakt: Value = serde_json::from_str(
            &trakt_watched_to_ids_json(r#"[{"movie":{"ids":{"tmdb":7}}}]"#, "[]")
                .expect("trakt watched"),
        )
        .unwrap();
        assert_eq!(trakt["tmdb:7"], Value::Bool(true));

        let simkl: Value = serde_json::from_str(
            &simkl_watched_to_ids_json("[]", r#"[{"movie":{"ids":{"tmdb":8}}}]"#)
                .expect("simkl watched"),
        )
        .unwrap();
        assert_eq!(simkl["tmdb:8"], Value::Bool(true));
    }

    #[test]
    fn history_request_builds_show_seasons_from_episode_ids() {
        let request = trakt_history_request_json(
            r#"{"id":"tt1234567","name":"Show","type":"series","poster":null}"#,
            r#"[{"id":"tt1234567:1:2","name":null,"season":null,"number":null,"released":null,"thumbnail":null}]"#,
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("history request");

        assert_eq!(
            request
                .get("shows")
                .and_then(Value::as_array)
                .and_then(|shows| shows.first())
                .and_then(|show| show.get("seasons"))
                .and_then(Value::as_array)
                .and_then(|seasons| seasons.first())
                .and_then(|season| season.get("number"))
                .and_then(Value::as_i64),
            Some(1)
        );
        assert!(request.get("movies").is_none());
    }

    #[test]
    fn trakt_oauth_error_code_extracts_structured_error() {
        assert_eq!(
            trakt_oauth_error_code(r#"{"error":"authorization_pending"}"#).as_deref(),
            Some("authorization_pending")
        );
        assert_eq!(trakt_oauth_error_code("{}"), None);
    }

    #[test]
    fn trakt_mark_watched_body_groups_episodes_by_show_and_dedupes() {
        let body = trakt_mark_watched_body_json(
            &json!([
                "tt1234567:1:1",
                "tt1234567:1:2",
                "tt1234567:1:1",
                "tt7654321"
            ])
            .to_string(),
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("body");

        let movies = body["movies"].as_array().unwrap();
        assert_eq!(movies.len(), 1);
        assert_eq!(movies[0]["ids"]["imdb"], "tt7654321");

        let shows = body["shows"].as_array().unwrap();
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0]["ids"]["imdb"], "tt1234567");
        let seasons = shows[0]["seasons"].as_array().unwrap();
        assert_eq!(seasons.len(), 1);
        assert_eq!(seasons[0]["number"], 1);
        // The duplicate tt1234567:1:1 must not produce a duplicate episode entry.
        let episodes = seasons[0]["episodes"].as_array().unwrap();
        assert_eq!(episodes.len(), 2);
        assert_eq!(episodes[0]["number"], 1);
        assert_eq!(episodes[1]["number"], 2);
    }

    #[test]
    fn trakt_mark_watched_body_is_none_for_unrecognized_ids() {
        assert_eq!(
            trakt_mark_watched_body_json(&json!(["not-an-id"]).to_string()),
            None
        );
    }

    #[test]
    fn timestamped_merge_pushes_removal_when_local_is_newer() {
        let local = json!([{"id": "a", "active": false, "updatedAt": 2000}]).to_string();
        let remote = json!([{"id": "a", "updatedAt": 1000}]).to_string();
        let result: Value =
            serde_json::from_str(&merge_watchlist_timestamped_json(&local, &remote)).unwrap();
        assert_eq!(result["toPushRemote"]["remove"], json!(["a"]));
        assert_eq!(result["toApplyLocal"]["add"], json!([]));
    }

    #[test]
    fn timestamped_merge_reapplies_locally_when_remote_is_newer() {
        let local = json!([{"id": "a", "active": false, "updatedAt": 1000}]).to_string();
        let remote = json!([{"id": "a", "updatedAt": 2000}]).to_string();
        let result: Value =
            serde_json::from_str(&merge_watchlist_timestamped_json(&local, &remote)).unwrap();
        assert_eq!(result["toApplyLocal"]["add"], json!(["a"]));
        assert_eq!(result["toPushRemote"]["remove"], json!([]));
    }

    #[test]
    fn timestamped_merge_pushes_local_only_additions() {
        let local = json!([{"id": "a", "active": true, "updatedAt": 1000}]).to_string();
        let remote = json!([]).to_string();
        let result: Value =
            serde_json::from_str(&merge_watchlist_timestamped_json(&local, &remote)).unwrap();
        assert_eq!(result["toPushRemote"]["add"], json!(["a"]));
    }

    #[test]
    fn anilist_save_media_list_entry_variables_parses_media_id() {
        let json =
            anilist_save_media_list_entry_variables_json("anilist:5", "PLANNING", None).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["mediaId"], json!(5));
        assert_eq!(value["status"], json!("PLANNING"));
        assert!(value.get("progress").is_none());
    }

    #[test]
    fn anilist_save_media_list_entry_variables_includes_progress_when_given() {
        let json = anilist_save_media_list_entry_variables_json("anilist:5", "COMPLETED", Some(12))
            .unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["progress"], json!(12));
    }

    #[test]
    fn anilist_save_media_list_entry_variables_rejects_non_anilist_ids() {
        assert_eq!(
            anilist_save_media_list_entry_variables_json("tt1234567", "PLANNING", None),
            None
        );
    }

    #[test]
    fn timestamped_merge_imports_remote_only_additions() {
        let local = json!([]).to_string();
        let remote = json!([{"id": "a", "updatedAt": 1000}]).to_string();
        let result: Value =
            serde_json::from_str(&merge_watchlist_timestamped_json(&local, &remote)).unwrap();
        assert_eq!(result["toApplyLocal"]["add"], json!(["a"]));
    }

    #[test]
    fn mal_sync_policy_maps_auth_and_episode_updates() {
        assert_eq!(
            external_sync_response_action("mal", 401),
            "refresh_credentials"
        );
        assert_eq!(
            external_sync_response_action("simkl", 401),
            "clear_credentials"
        );
        assert_eq!(
            external_sync_refresh_retry_action(Some(401)),
            "clear_credentials"
        );
        let watched = mal_list_update_json(
            &json!({
                "meta": { "id": "mal:42", "type": "series", "episodesCount": 12 },
                "episodes": [{ "number": 12 }],
            })
            .to_string(),
            true,
        )
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap();
        assert_eq!(watched["status"], "completed");
    }

    #[test]
    fn simkl_request_policy_builds_series_history_and_watchlist_removal() {
        let history = simkl_history_request_json(
            &json!({
                "imdbId": "tt1",
                "isSeries": true,
                "episodesBySeasonNumber": { "2": [3, 4] },
            })
            .to_string(),
        )
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap();
        assert_eq!(history["shows"][0]["seasons"][0]["number"], 2);
        assert_eq!(
            history["shows"][0]["seasons"][0]["episodes"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );

        let removal = simkl_watchlist_request_json(
            &json!({ "imdbId": "tt1", "isSeries": false }).to_string(),
            true,
        )
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap();
        assert!(removal["movies"][0].get("to").is_none());
    }

    #[test]
    fn trakt_sync_item_policy_normalizes_identity_and_release_date() {
        let meta = trakt_sync_item_to_meta_json(
            &json!({
                "item": { "show": { "title": "Show", "year": 2025, "ids": { "imdb": "tt1" } } },
                "type": "series",
                "unknownName": "Unknown",
            })
            .to_string(),
        )
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .unwrap();
        assert_eq!(meta["id"], "tt1");
        assert_eq!(meta["released"], "2025-01-01");
    }

    #[test]
    fn trakt_playback_deletion_matches_shared_content_identity() {
        let ids: Value = serde_json::from_str(&trakt_playback_delete_ids_json(&json!({
            "contentId":"tmdb:42",
            "items":[{"id":1,"show":{"ids":{"tmdb":42}}},{"id":2,"movie":{"ids":{"imdb":"tt1"}}}],
        }).to_string()).unwrap()).unwrap();
        assert_eq!(ids, json!([1]));
    }

    #[test]
    fn trakt_activity_diff_flags_only_moved_timestamps() {
        let result: Value = serde_json::from_str(&trakt_activity_diff_json(&json!({
            "previous": { "movies": { "paused_at": "t1", "watchlisted_at": "t1", "watched_at": "t1" }, "episodes": { "paused_at": "t1", "watched_at": "t1" }, "shows": { "watchlisted_at": "t1" } },
            "current": { "movies": { "paused_at": "t1", "watchlisted_at": "t1", "watched_at": "t2" }, "episodes": { "paused_at": "t1", "watched_at": "t1" }, "shows": { "watchlisted_at": "t1" } },
            "hasPlayback": true,
            "hasWatchlistMovies": true,
            "hasWatchlistShows": true,
            "hasWatchedMovies": true,
            "hasWatchedShows": true,
        }).to_string()).unwrap()).unwrap();
        assert_eq!(result["playbackChanged"], false);
        assert_eq!(result["watchlistMoviesChanged"], false);
        assert_eq!(result["watchlistShowsChanged"], false);
        assert_eq!(result["watchedMoviesChanged"], true);
        assert_eq!(result["watchedShowsChanged"], false);
    }

    #[test]
    fn trakt_activity_diff_forces_full_when_nothing_cached_yet() {
        let result: Value = serde_json::from_str(&trakt_activity_diff_json(&json!({
            "previous": null,
            "current": { "movies": {}, "episodes": {}, "shows": {} },
            "hasPlayback": false,
            "hasWatchlistMovies": false,
            "hasWatchlistShows": false,
            "hasWatchedMovies": false,
            "hasWatchedShows": false,
        }).to_string()).unwrap()).unwrap();
        assert_eq!(result["playbackChanged"], true);
        assert_eq!(result["watchedShowsChanged"], true);
    }

    #[test]
    fn simkl_resource_sync_plan_marks_removed_from_list_as_force_full() {
        let plan: Value = serde_json::from_str(&simkl_resource_sync_plan_json(&json!({
            "previous": { "tv_shows": { "watching": "t1", "removed_from_list": "r1" } },
            "current": { "tv_shows": { "watching": "t1", "removed_from_list": "r2" } },
            "resources": [{ "key": "showsWatching", "type": "tv_shows", "status": "watching", "hasCached": true }],
        }).to_string()).unwrap()).unwrap();
        assert_eq!(plan[0]["action"], "full");
    }

    #[test]
    fn simkl_resource_sync_plan_uses_date_from_for_partial_delta() {
        let plan: Value = serde_json::from_str(&simkl_resource_sync_plan_json(&json!({
            "previous": { "tv_shows": { "watching": "t1", "removed_from_list": "r1" } },
            "current": { "tv_shows": { "watching": "t2", "removed_from_list": "r1" } },
            "resources": [{ "key": "showsWatching", "type": "tv_shows", "status": "watching", "hasCached": true }],
        }).to_string()).unwrap()).unwrap();
        assert_eq!(plan[0]["action"], "delta");
        assert_eq!(plan[0]["dateFrom"], "t1");
    }

    #[test]
    fn simkl_merge_delta_updates_existing_and_appends_new_items() {
        let merged: Value = serde_json::from_str(&simkl_merge_delta_json(
            &json!([{ "ids": { "simkl": 1 }, "progress": 10 }, { "ids": { "simkl": 2 }, "progress": 20 }]).to_string(),
            &json!([{ "ids": { "simkl": 1 }, "progress": 50 }, { "ids": { "simkl": 3 }, "progress": 5 }]).to_string(),
        ).unwrap()).unwrap();
        assert_eq!(merged, json!([
            { "ids": { "simkl": 1 }, "progress": 50 },
            { "ids": { "simkl": 2 }, "progress": 20 },
            { "ids": { "simkl": 3 }, "progress": 5 },
        ]));
    }
}
