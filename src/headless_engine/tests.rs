use super::*;
use serde_json::json;

#[test]
fn detail_load_emits_platform_effects_and_completion_updates_state() {
    let handle = create_headless_engine("{}");
    let result: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1","language":"en"}"#,
        )
        .expect("dispatch"),
    )
    .expect("json");

    assert_eq!(result["state"]["detail"]["isLoading"], true);
    assert_eq!(result["effects"][0]["type"], "fetchMetaDetail");
    assert_eq!(result["effects"][1]["type"], "readPlaybackProgress");

    let effect_id = result["effects"][0]["id"].as_str().unwrap();
    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": { "id": "tt1", "name": "Movie" }
            })
            .to_string(),
        )
        .expect("complete"),
    )
    .expect("json");

    assert_eq!(completed["state"]["detail"]["isLoading"], false);
    assert_eq!(completed["state"]["detail"]["meta"]["name"], "Movie");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn concurrent_local_state_request_does_not_drop_meta_detail_completion() {
    let handle = create_headless_engine("{}");
    let load: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"detailLoadRequested","contentType":"series","id":"tt1"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let effect_id = load["effects"][0]["id"].as_str().unwrap().to_string();

    headless_engine_dispatch_json(
        handle,
        r#"{"type":"detailLocalStateRequested","primaryId":"tt1","contentType":"series"}"#,
    )
    .unwrap();

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": {
                    "id": "tt1",
                    "name": "Rick and Morty",
                    "videos": [{"id": "tt1:1:1", "season": 1, "episode": 1}]
                }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        completed["state"]["detail"]["meta"]["name"],
        "Rick and Morty"
    );
    assert_eq!(
        completed["state"]["detail"]["meta"]["videos"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(destroy_headless_engine(handle));
}

#[test]
fn detail_meta_trailers_are_normalized_in_core_before_tmdb_fallback() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1","language":"en"}"#,
        )
        .expect("dispatch"),
    )
    .expect("json");
    let effect_id = requested["effects"][0]["id"].as_str().unwrap();
    let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": effect_id,
                    "status": "ok",
                    "value": {
                        "id": "tt1",
                        "name": "Movie",
                        "trailers": [
                            { "source": "abc123", "type": "Trailer" },
                            { "title": "Featurette", "url": "https://video.example/f.mp4", "type": "Clip" }
                        ]
                    }
                })
                .to_string(),
            )
            .expect("complete"),
        )
        .expect("json");

    assert_eq!(
        completed["state"]["detail"]["trailers"][0]["url"],
        "https://www.youtube.com/watch?v=abc123"
    );
    assert_eq!(
        completed["state"]["detail"]["trailers"][1]["title"],
        "Featurette"
    );

    assert!(destroy_headless_engine(handle));
}

#[test]
fn detail_meta_link_trailers_become_direct_playback_sources() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailLoadRequested","contentType":"series","id":"tt0944947","language":"en"}"#,
            )
            .expect("dispatch"),
        )
        .expect("json");
    let effect_id = requested["effects"][0]["id"].as_str().unwrap();
    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": {
                    "id": "tt0944947",
                    "name": "Game of Thrones",
                    "links": [
                        {
                            "trailers": "https://video.fandango.com/trailer.mp4",
                            "provider": "Rotten Tomatoes 1080p"
                        },
                        {
                            "trailers": "https://imdb-video.media-imdb.com/trailer.m3u8",
                            "provider": "IMDb SD"
                        }
                    ]
                }
            })
            .to_string(),
        )
        .expect("complete"),
    )
    .expect("json");

    assert_eq!(
        completed["state"]["detail"]["trailers"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        completed["state"]["detail"]["trailers"][0]["title"],
        "Rotten Tomatoes 1080p"
    );
    assert_eq!(
        completed["state"]["detail"]["trailers"][1]["url"],
        "https://imdb-video.media-imdb.com/trailer.m3u8"
    );

    assert!(destroy_headless_engine(handle));
}

#[test]
fn detail_selected_addon_changes_visible_streams_without_mutating_raw_streams() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"detailStreamsRequested","contentType":"movie","requestIds":["tt1"],"detail":null,"seasonEpisodes":[],"language":"en"}"#,
            )
            .expect("dispatch"),
        )
        .expect("json");
    let effect_id = requested["effects"][0]["id"].as_str().unwrap();
    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": {
                    "streams": [
                        { "title": "A", "addonName": "One" },
                        { "title": "B", "addonName": "Two" },
                        { "title": "C", "addonName": "One" }
                    ],
                    "availableAddons": ["One", "Two"],
                    "hasStreamProviders": true
                }
            })
            .to_string(),
        )
        .expect("complete"),
    )
    .expect("json");
    assert_eq!(completed["state"]["detail"]["streams"][0]["title"], "A");
    assert_eq!(
        completed["state"]["detail"]["visibleStreams"][1]["title"],
        "B"
    );

    let selected: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"detailSelectedAddonChanged","addon":"one"}"#,
        )
        .expect("dispatch"),
    )
    .expect("json");

    assert_eq!(
        selected["state"]["detail"]["streams"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        selected["state"]["detail"]["visibleStreams"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        selected["state"]["detail"]["visibleStreams"][0]["title"],
        "A"
    );
    assert_eq!(
        selected["state"]["detail"]["visibleStreams"][1]["title"],
        "C"
    );
    assert!(destroy_headless_engine(handle));
}

#[test]
fn stale_detail_effect_completion_does_not_override_newer_state() {
    let handle = create_headless_engine("{}");
    let first: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let stale_effect_id = first["effects"][0]["id"].as_str().unwrap().to_string();

    headless_engine_dispatch_json(
        handle,
        r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt2"}"#,
    )
    .unwrap();

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": stale_effect_id,
                "status": "ok",
                "value": { "id": "tt1", "name": "Old" }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    // Stale completion is ignored, so this dispatch's patch doesn't touch detail at all —
    // its absence here is what proves tt2's state wasn't overridden by tt1's late result.
    assert!(completed["state"]["detail"].is_null());
    assert!(destroy_headless_engine(handle));
}

#[test]
fn player_load_streams_uses_effect_completion_without_reordering_streams() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerLoadStreamsRequested","contentType":"movie","id":"tt1","currentVideoId":"tt1","initialStreamIndex":1}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(requested["effects"][0]["type"], "loadStreams");

    let effect_id = requested["effects"][0]["id"].as_str().unwrap();
    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": [
                    { "title": "A", "playableUrl": "http://a" },
                    { "title": "B", "playableUrl": "http://b" }
                ]
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(completed["state"]["player"]["currentStreamIndex"], 1);
    assert_eq!(completed["state"]["player"]["currentUrl"], "http://b");
    assert_eq!(
        completed["state"]["player"]["currentStreams"][0]["title"],
        "A"
    );
    assert!(destroy_headless_engine(handle));
}

#[test]
fn player_load_streams_saves_outgoing_episode_progress_before_switching() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            &json!({
                "type": "playerLoadStreamsRequested",
                "contentType": "series",
                "id": "tt1",
                "currentVideoId": "tt1:1:5",
                "initialVideoId": "tt1:1:6",
                "outgoingProgress": {
                    "timeOffset": 1200000,
                    "duration": 1300000,
                    "lastEpisodeSeason": 1,
                    "lastEpisodeNumber": 5
                }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    let effects = requested["effects"].as_array().unwrap();
    assert!(effects.iter().any(|e| e["type"] == "writePlaybackProgress"
        && e["payload"]["progress"]["lastVideoId"] == "tt1:1:5"
        && e["payload"]["progress"]["lastEpisodeNumber"] == 5));
    assert!(effects.iter().any(|e| e["type"] == "loadStreams"));
    assert!(destroy_headless_engine(handle));
}

#[test]
fn player_resolve_playback_emits_torrent_or_direct_platform_effects() {
    let handle = create_headless_engine("{}");
    let torrent: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerResolvePlaybackRequested","url":"stremio://torrent/abc","stream":{"title":"T"},"currentVideoId":"tt1","title":"Movie"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(torrent["effects"][0]["type"], "startTorrentStream");
    let effect_id = torrent["effects"][0]["id"].as_str().unwrap();

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": { "url": "http://127.0.0.1:8090/stream" }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        completed["state"]["player"]["resolvedUrl"],
        "http://127.0.0.1:8090/stream"
    );

    let direct: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerResolvePlaybackRequested","url":"https://video.example/file.mp4","title":"Movie"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        direct["state"]["player"]["resolvedUrl"],
        "https://video.example/file.mp4"
    );
    assert_eq!(direct["effects"][0]["type"], "stopTorrent");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn home_load_is_owned_by_core_and_resolved_through_platform_effect() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"homeLoadRequested","profile":{"id":"p1"},"language":"tr","force":true}"#,
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(requested["state"]["home"]["isLoading"], true);
    assert_eq!(requested["effects"][0]["type"], "readHomeBootstrap");
    assert_eq!(requested["effects"][0]["payload"]["profileId"], "p1");
    assert_eq!(requested["effects"][0]["payload"]["language"], "tr");
    assert_eq!(requested["effects"][0]["payload"]["force"], true);

    let effect_id = requested["effects"][0]["id"].as_str().unwrap();
    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": {
                    "categories": [{ "id": "featured" }],
                    "continueWatching": [{ "id": "tt1" }],
                    "metadataFeeds": [{ "key": "cinemeta" }],
                    "billboard": { "id": "tt2" }
                }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(completed["state"]["home"]["isLoading"], false);
    assert_eq!(
        completed["state"]["home"]["categories"][0]["id"],
        "featured"
    );
    assert_eq!(
        completed["state"]["home"]["continueWatching"][0]["id"],
        "tt1"
    );
    assert_eq!(completed["state"]["home"]["billboard"]["id"], "tt2");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn forced_home_refresh_keeps_stale_categories_visible() {
    let handle = create_headless_engine("{}");
    let initial: Value = serde_json::from_str(
        &headless_engine_dispatch_json(handle, r#"{"type":"homeLoadRequested"}"#).unwrap(),
    )
    .unwrap();
    let cached: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": initial["effects"][0]["id"],
                "status": "ok",
                "value": { "stale": true, "categories": [{ "id": "cached" }] }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(cached["state"]["home"]["isStale"], true);
    assert_eq!(cached["state"]["home"]["categories"][0]["id"], "cached");

    let refresh: Value = serde_json::from_str(
        &headless_engine_dispatch_json(handle, r#"{"type":"homeLoadRequested","force":true}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(refresh["state"]["home"]["isLoading"], true);
    assert_eq!(refresh["state"]["home"]["isStale"], false);
    assert_eq!(refresh["state"]["home"]["categories"][0]["id"], "cached");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn home_load_delivers_continue_watching_in_its_single_bootstrap_effect() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"homeLoadRequested","profile":{"id":"p1"},"language":"tr"}"#,
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(requested["effects"].as_array().unwrap().len(), 1);
    assert_eq!(requested["effects"][0]["type"], "readHomeBootstrap");
    assert_eq!(requested["effects"][0]["payload"]["profileId"], "p1");
    assert_eq!(requested["effects"][0]["payload"]["language"], "tr");

    let bootstrap_id = requested["effects"][0]["id"].as_str().unwrap();
    let bootstrap_completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": bootstrap_id,
                    "status": "ok",
                    "value": { "continueWatching": [{ "id": "tt1", "continueWatchingBadge": "newEpisode" }] }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(bootstrap_completed["state"]["home"]["isLoading"], false);
    assert_eq!(
        bootstrap_completed["state"]["home"]["continueWatching"][0]["continueWatchingBadge"],
        "newEpisode"
    );
    assert_eq!(
        bootstrap_completed["state"]["home"]["continueWatching"][0]["continueWatchingBadge"],
        "newEpisode"
    );
    assert!(destroy_headless_engine(handle));
}

#[test]
fn library_commands_are_storage_effects_owned_by_core() {
    let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);
    let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"toggleWatchlistRequested","profile":{"id":"p2"},"item":{"id":"tt1","name":"Movie","type":"movie"}}"#,
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(requested["effects"][0]["type"], "writeLibraryCommand");
    assert_eq!(requested["effects"][0]["payload"]["profileId"], "p2");
    assert_eq!(
        requested["effects"][0]["payload"]["command"]["type"],
        "toggleWatchlist"
    );
    assert_eq!(
        requested["effects"][0]["payload"]["command"]["item"]["id"],
        "tt1"
    );

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                "status": "ok",
                "value": { "watchlist": [{ "id": "tt1" }] }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    assert!(completed["state"]["library"]["lastWriteError"].is_null());
    assert_eq!(
        completed["state"]["library"]["lastWrite"]["watchlist"][0]["id"],
        "tt1"
    );
    assert!(destroy_headless_engine(handle));
}

#[test]
fn playback_progress_write_is_clamped_and_delegated_to_storage_adapter() {
    let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);
    let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"savePlaybackProgressRequested","meta":{"id":"tt1","name":"Movie","type":"movie"},"timeOffset":-10,"duration":7200,"lastVideoId":"tt1","lastStreamIndex":2,"lastEpisodeName":null,"lastStreamUrl":"http://a","lastStreamTitle":"A","lastAudioLanguage":"en","lastSubtitleLanguage":"tr"}"#,
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(requested["effects"][0]["type"], "writePlaybackProgress");
    assert_eq!(requested["effects"][0]["payload"]["profileId"], "p1");
    assert_eq!(
        requested["effects"][0]["payload"]["progress"]["timeOffset"],
        0
    );
    assert_eq!(
        requested["effects"][0]["payload"]["progress"]["lastSubtitleLanguage"],
        "tr"
    );

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                "status": "ok",
                "value": {}
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    assert!(completed["state"]["library"]["pendingPlaybackProgress"].is_null());
    assert_eq!(
        completed["state"]["library"]["savedPlaybackProgress"]["meta"]["id"],
        "tt1"
    );
    assert!(destroy_headless_engine(handle));
}

#[test]
fn clearing_playback_progress_drops_the_item_from_home_continue_watching() {
    let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);
    headless_engine_dispatch_json(
        handle,
        r#"{"type":"homeLoadRequested","profile":{"id":"p1"}}"#,
    )
    .unwrap();
    let home_loaded: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": "fx-1",
                "status": "ok",
                "value": { "continueWatching": [{ "id": "tt1" }, { "id": "tt2" }] }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        home_loaded["state"]["home"]["continueWatching"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let requested: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"clearPlaybackProgressRequested","meta":{"id":"tt1"}}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let effect_id = requested["effects"][0]["id"].as_str().unwrap();

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": { "droppedId": "tt1" }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    let continue_watching = completed["state"]["home"]["continueWatching"]
        .as_array()
        .unwrap();
    assert_eq!(continue_watching.len(), 1);
    assert_eq!(continue_watching[0]["id"], "tt2");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn completing_an_effect_does_not_redeliver_already_delivered_siblings() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1","language":"en"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    // dispatch_load creates and delivers both effects directly in one response.
    assert_eq!(requested["effects"][0]["type"], "fetchMetaDetail");
    assert_eq!(requested["effects"][1]["type"], "readPlaybackProgress");

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                "status": "ok",
                "value": { "id": "tt1", "name": "Movie" }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    // readPlaybackProgress was already handed to the platform alongside fetchMetaDetail.
    // Completing fetchMetaDetail must not hand it out again as if it were fresh work —
    // the platform is presumably still executing it.
    assert!(completed["effects"].as_array().unwrap().is_empty());
    assert!(destroy_headless_engine(handle));
}

#[test]
fn expire_stale_pending_effects_drops_old_but_not_recent_effects() {
    let mut engine = HeadlessEngine::default();
    let action: AppAction = serde_json::from_str(
        r#"{"type":"detailLoadRequested","contentType":"movie","id":"tt1","language":"en"}"#,
    )
    .unwrap();
    let effects = engine.dispatch(action);
    let visible = engine.resolve_visible_effects(effects);
    assert_eq!(visible.len(), 2);

    // Still well within the window — nothing genuinely in flight should be dropped.
    engine.expire_stale_pending_effects(Instant::now());
    assert_eq!(engine.state.pending_effects.len(), 2);

    // Past the expiry window — abandoned effects (platform never called
    // complete_effect) get swept from all three bookkeeping collections.
    let far_future = Instant::now() + Duration::from_secs(301);
    engine.expire_stale_pending_effects(far_future);
    assert!(engine.state.pending_effects.is_empty());
    assert!(engine.delivered_effect_ids.is_empty());
    assert!(engine.effect_created_at.is_empty());
}

#[test]
fn addon_search_discover_and_catalog_backbone_are_effect_driven() {
    let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);

    let addon: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"addonInstallRequested","transportUrl":"https://addon.example/manifest.json","forceRefresh":true}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(addon["effects"][0]["type"], "fetchAddonManifest");
    assert_eq!(
        addon["effects"][0]["payload"]["transportUrl"],
        "https://addon.example/manifest.json"
    );

    let completed_addon: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": addon["effects"][0]["id"].as_str().unwrap(),
                "status": "ok",
                "value": {
                    "id": "addon.example",
                    "transportUrl": "https://addon.example/manifest.json",
                    "name": "Addon"
                }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        completed_addon["state"]["addons"]["installed"][0]["name"],
        "Addon"
    );

    let resource: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"addonResourceRequested","transportUrl":"https://addon.example/manifest.json","resource":"stream","contentType":"movie","id":"tt1","extra":{"search":"keep order"}}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(resource["effects"][0]["type"], "fetchAddonResource");
    assert_eq!(resource["effects"][0]["payload"]["resource"], "stream");
    assert_eq!(
        resource["effects"][0]["payload"]["extra"]["search"],
        "keep order"
    );

    let search: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"searchRequested","query":"matrix","language":"en"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(search["effects"][0]["type"], "runSearch");
    assert_eq!(search["effects"][0]["payload"]["profileId"], "p1");

    let discover: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"discoverRequested","contentType":"movie","filters":{"genre":"sci-fi"},"language":"en"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(discover["effects"][0]["type"], "runDiscover");
    assert_eq!(
        discover["effects"][0]["payload"]["filters"]["genre"],
        "sci-fi"
    );

    let page: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"catalogPageRequested","categoryId":"cat","transportUrl":"https://addon.example/manifest.json","contentType":"movie","catalogId":"top","skip":-10,"genre":null,"search":null}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(page["effects"][0]["type"], "fetchCatalogPage");
    assert_eq!(page["effects"][0]["payload"]["skip"], 0);
    assert!(destroy_headless_engine(handle));
}

#[test]
fn concurrent_catalog_filters_request_does_not_drop_discover_results() {
    let handle = create_headless_engine("{}");
    let discover: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"discoverRequested","contentType":"movie","filters":{"catalogKey":"top"}}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let effect_id = discover["effects"][0]["id"].as_str().unwrap().to_string();

    headless_engine_dispatch_json(
            handle,
            r#"{"type":"discoverCatalogFiltersRequested","contentType":"movie","selectedCatalogKey":"top"}"#,
        )
        .unwrap();

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": { "results": [{"id": "tt1", "type": "movie", "name": "A Movie"}] }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(completed["state"]["discover"]["results"][0]["id"], "tt1");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn discover_prefetches_two_pages_in_one_round_trip() {
    let handle = create_headless_engine(
        r#"{"discover":{"catalogs":[{"key":"top","transportUrl":"https://addon.example/manifest.json","id":"top","type":"movie"}]}}"#,
    );
    let discover: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"discoverRequested","contentType":"movie","filters":{"catalogKey":"top","extra":{"genre":"action"}}}"#,
            )
            .unwrap(),
        )
        .unwrap();
    let first_page: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": discover["effects"][0]["id"].as_str().unwrap(),
                "status": "ok",
                "value": { "results": [{ "id": "tt1" }] }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        first_page["state"]["discover"]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let effects = first_page["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0]["type"], "fetchDiscoverPage");
    assert_eq!(effects[0]["payload"]["skip"], 20);
    assert_eq!(effects[1]["type"], "fetchDiscoverPage");
    assert_eq!(effects[1]["payload"]["skip"], 40);

    let second_page: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effects[0]["id"].as_str().unwrap(),
                "status": "ok",
                "value": { "items": [{ "id": "tt2" }] }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        second_page["state"]["discover"]["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let third_page: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effects[1]["id"].as_str().unwrap(),
                "status": "ok",
                "value": { "items": [{ "id": "tt3" }] }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        third_page["state"]["discover"]["results"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(destroy_headless_engine(handle));
}

#[test]
fn detail_player_sync_auth_settings_calendar_and_offline_are_core_actions() {
    let handle = create_headless_engine(r#"{"profile":{"activeProfileId":"p1"}}"#);

    let season: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"detailSeasonRequested","seriesId":"tt1","season":2}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(season["effects"][0]["type"], "fetchSeasonEpisodes");

    let subtitles: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"subtitleLoadRequested","stream":{"url":"http://a"},"contentType":"movie","id":"tt1","extraArgs":"videoHash=abc"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(subtitles["effects"][0]["type"], "fetchSubtitles");
    assert_eq!(
        subtitles["effects"][0]["payload"]["extraArgs"],
        "videoHash=abc"
    );

    let sync: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"externalSyncRequested","provider":"trakt","language":"tr"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(sync["effects"][0]["type"], "runExternalSync");
    assert_eq!(sync["effects"][0]["payload"]["profileId"], "p1");

    let auth: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"authFlowRequested","provider":"trakt","mode":"deviceCode"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(auth["effects"][0]["type"], "runAuthFlow");

    let settings: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"settingsChanged","key":"language","value":"tr"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(settings["state"]["settings"]["values"]["language"], "tr");
    assert_eq!(settings["effects"][0]["type"], "writeSettings");

    let calendar: Value = serde_json::from_str(
        &headless_engine_dispatch_json(
            handle,
            r#"{"type":"calendarMonthRequested","year":2026,"month":20}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(calendar["effects"][0]["type"], "readCalendarMonth");
    assert_eq!(calendar["effects"][0]["payload"]["month"], 12);

    let offline: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"offlineDownloadRequested","meta":{"id":"tt1"},"stream":{"url":"http://a"},"videoId":"tt1"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(offline["effects"][0]["type"], "enqueueOfflineDownload");
    assert_eq!(offline["effects"][0]["payload"]["meta"]["id"], "tt1");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn plugin_repository_add_completion_populates_repositories_and_scrapers() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"pluginRepositoryAddRequested","manifestUrl":"https://example.com/manifest.json"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(requested["effects"][0]["type"], "fetchPluginManifest");
    assert_eq!(
        requested["state"]["plugins"]["addingRepositoryUrl"],
        "https://example.com/manifest.json"
    );

    let completed: Value = serde_json::from_str(
            &headless_engine_complete_effect_json(
                handle,
                &json!({
                    "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                    "status": "ok",
                    "value": {
                        "manifestUrl": "https://example.com/manifest.json",
                        "manifest": {
                            "name": "Phisher's Repo",
                            "version": "1.0.0",
                            "scrapers": [
                                {"id": "MoviesDrive", "name": "MoviesDrive", "version": "1.1.1", "filename": "src/providers/moviesdrive.js"}
                            ]
                        }
                    }
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(
        completed["state"]["plugins"]["addingRepositoryUrl"],
        Value::Null
    );
    assert_eq!(
        completed["state"]["plugins"]["repositories"][0]["name"],
        "Phisher's Repo"
    );
    assert_eq!(
        completed["state"]["plugins"]["scrapers"][0]["repositoryUrl"],
        "https://example.com/manifest.json"
    );

    let removed: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"pluginRepositoryRemoveRequested","manifestUrl":"https://example.com/manifest.json"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        removed["state"]["plugins"]["repositories"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        removed["state"]["plugins"]["scrapers"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert!(destroy_headless_engine(handle));
}

#[test]
fn plugin_repository_refetch_preserves_disabled_state_and_settings() {
    let handle = create_headless_engine("{}");
    let manifest_value = json!({
        "manifestUrl": "https://example.com/manifest.json",
        "manifest": {
            "name": "Phisher's Repo",
            "version": "1.0.0",
            "scrapers": [
                {"id": "MoviesDrive", "name": "MoviesDrive", "version": "1.1.1", "filename": "src/providers/moviesdrive.js"}
            ]
        }
    });

    let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"pluginRepositoryAddRequested","manifestUrl":"https://example.com/manifest.json"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    headless_engine_complete_effect_json(
        handle,
        &json!({
            "effectId": requested["effects"][0]["id"].as_str().unwrap(),
            "status": "ok",
            "value": manifest_value
        })
        .to_string(),
    )
    .unwrap();

    headless_engine_dispatch_json(
        handle,
        r#"{"type":"pluginScraperToggled","scraperId":"MoviesDrive","enabled":false}"#,
    )
    .unwrap();
    headless_engine_dispatch_json(
            handle,
            r#"{"type":"pluginScraperSettingsUpdated","scraperId":"MoviesDrive","settings":{"quality":"1080p"}}"#,
        )
        .unwrap();

    let refetch_requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"pluginRepositoryAddRequested","manifestUrl":"https://example.com/manifest.json"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    let refetched: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": refetch_requested["effects"][0]["id"].as_str().unwrap(),
                "status": "ok",
                "value": manifest_value
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    let scraper = &refetched["state"]["plugins"]["scrapers"][0];
    assert_eq!(scraper["enabled"], false);
    assert_eq!(scraper["settings"]["quality"], "1080p");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn calendar_completion_plans_os_side_effects_in_core() {
    let handle = create_headless_engine("{}");
    let requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"calendarMonthRequested","profile":{"id":"p1","language":"tr"},"year":2026,"month":5}"#,
            )
            .unwrap(),
        )
        .unwrap();

    let completed: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": requested["effects"][0]["id"].as_str().unwrap(),
                "status": "ok",
                "value": {
                    "items": [{ "dateIso": "2026-05-20", "title": "Episode" }],
                    "localItems": [{ "id": "tt1" }],
                    "externalItems": [{ "id": "tt2" }]
                }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(completed["state"]["calendar"]["isLoading"], false);
    assert_eq!(
        completed["state"]["calendar"]["items"][0]["title"],
        "Episode"
    );
    assert_eq!(
        completed["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|effect| effect["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "updateCalendarWidget",
            "notifyReleasedEpisodes",
            "replaceExternalContinueWatching"
        ]
    );
    assert_eq!(completed["effects"][0]["payload"]["profile"]["id"], "p1");
    assert_eq!(completed["effects"][2]["payload"]["items"][0]["id"], "tt2");
    assert!(destroy_headless_engine(handle));
}

#[test]
fn next_episode_card_shown_prefetches_streams_and_load_streams_consumes_cache() {
    let handle = create_headless_engine("{}");

    // 1. Next episode card shown for episode tt1:1:2
    let prefetch_requested: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerNextEpisodeCardShown","contentType":"series","seriesId":"tt1","nextVideoId":"tt1:1:2","title":"Show","language":"en"}"#,
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(
        prefetch_requested["effects"][0]["type"],
        "prefetchNextEpisodeStreams"
    );
    assert_eq!(
        prefetch_requested["effects"][0]["payload"]["nextVideoId"],
        "tt1:1:2"
    );
    assert_eq!(
        prefetch_requested["state"]["player"]["prefetchingNextVideoId"],
        "tt1:1:2"
    );

    // Duplicate card-shown dispatch must not change prefetching state.
    let duplicate: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerNextEpisodeCardShown","contentType":"series","seriesId":"tt1","nextVideoId":"tt1:1:2"}"#,
            )
            .unwrap(),
        )
        .unwrap();
    // Guard works: nothing in player changed, so it's correctly absent from this patch
    // entirely (no new prefetch effect was queued either).
    assert!(duplicate["state"]["player"].is_null());

    // 2. Platform completes the prefetch with streams for tt1:1:2
    let effect_id = prefetch_requested["effects"][0]["id"].as_str().unwrap();
    let prefetch_done: Value = serde_json::from_str(
        &headless_engine_complete_effect_json(
            handle,
            &json!({
                "effectId": effect_id,
                "status": "ok",
                "value": {
                    "streams": [
                        { "title": "S", "playableUrl": "http://ep2" }
                    ]
                }
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        prefetch_done["state"]["player"]["prefetchedNextEpisode"]["videoId"],
        "tt1:1:2"
    );
    assert_eq!(
        prefetch_done["state"]["player"]["prefetchedNextEpisode"]["streams"][0]["title"],
        "S"
    );
    assert!(prefetch_done["state"]["player"]["prefetchingNextVideoId"].is_null());

    // 3. User navigates to ep2 — load streams without passing initial_streams.
    //    Core must inject the prefetched streams and use_initial_streams = true.
    let load: Value = serde_json::from_str(
            &headless_engine_dispatch_json(
                handle,
                r#"{"type":"playerLoadStreamsRequested","contentType":"series","id":"tt1","currentVideoId":"tt1:1:2"}"#,
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(load["effects"][0]["type"], "loadStreams");
    // useInitialStreams = true means the platform skips the network fetch
    assert_eq!(load["effects"][0]["payload"]["useInitialStreams"], true);
    // Cache must be consumed (cleared) after use
    assert!(load["state"]["player"]["prefetchedNextEpisode"].is_null());

    assert!(destroy_headless_engine(handle));
}

#[test]
fn engines_lock_survives_a_panic_while_held_by_another_thread() {
    // Poison the lock the same way a caught panic in a request would: a
    // thread panics while still holding the guard.
    let poisoner = std::thread::spawn(|| {
        let _guard = engines().lock().unwrap();
        panic!("simulated panic while holding the engines lock");
    });
    assert!(poisoner.join().is_err());

    // A naive `.lock().ok()` would now return None forever; lock_engines
    // must recover the guard so the store keeps working.
    let handle = create_headless_engine("{}");
    assert!(handle > 0);
    assert!(headless_engine_snapshot_json(handle).is_some());
    assert!(destroy_headless_engine(handle));
}

#[test]
fn wire_fixtures_match_golden_dispatch_output() {
    let actions_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wire/actions");
    let expected_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wire/expected");
    let update = std::env::var("UPDATE_WIRE_FIXTURES").is_ok();

    let mut entries: Vec<_> = std::fs::read_dir(&actions_dir)
        .expect("tests/wire/actions must exist")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no wire fixtures found");

    for path in entries {
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let fixture: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let initial_state = serde_json::to_string(&fixture["initialState"]).unwrap();
        let action_json = serde_json::to_string(&fixture["action"]).unwrap();

        let handle = create_headless_engine(&initial_state);
        let actual: Value = serde_json::from_str(
            &headless_engine_dispatch_json(handle, &action_json)
                .unwrap_or_else(|| panic!("dispatch failed for fixture {name}")),
        )
        .unwrap();
        destroy_headless_engine(handle);

        let expected_path = expected_dir.join(format!("{name}.json"));
        if update {
            std::fs::create_dir_all(&expected_dir).unwrap();
            std::fs::write(
                &expected_path,
                serde_json::to_string_pretty(&actual).unwrap() + "\n",
            )
            .unwrap();
            continue;
        }

        let expected: Value = serde_json::from_str(&std::fs::read_to_string(&expected_path)
                .unwrap_or_else(|_| {
                    panic!("missing golden fixture {expected_path:?}; run with UPDATE_WIRE_FIXTURES=1 to generate")
                }))
            .unwrap();

        assert_eq!(actual, expected, "wire fixture drift in {name}");
    }
}
