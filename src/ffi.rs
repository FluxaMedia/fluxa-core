use serde_json::{json, Value};

use crate::{
    addon_protocol, addon_resource, anime_detection, app_state, calendar_plan, content_identity,
    core_contract, external_sync, headless_engine, home_ranking, intro_segments, library_state,
    offline_download, platform_plan, player_policy, player_scrobble, repository_flow, search_plan,
    stream_policy, tmdb_plan, watchlist_plan,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    UnknownMethod,
    InvalidArgs,
    NotFound,
    Internal,
}

impl ErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            ErrorKind::UnknownMethod => "unknown_method",
            ErrorKind::InvalidArgs => "invalid_args",
            ErrorKind::NotFound => "not_found",
            ErrorKind::Internal => "internal",
        }
    }
}

struct CallError {
    kind: ErrorKind,
    message: String,
}

fn fail(kind: ErrorKind, message: impl Into<String>) -> CallError {
    CallError {
        kind,
        message: message.into(),
    }
}

type Outcome = Result<Value, CallError>;

pub fn core_invoke(method: &str, args_json: &str) -> String {
    // A panic anywhere in route()/the domain modules must not take the host
    // process down with it — catch it here and hand back the same error
    // envelope shape callers already handle for any other failure.
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| route(method, args_json)));
    match outcome {
        Ok(Ok(value)) => json!({ "ok": true, "value": value }).to_string(),
        Ok(Err(e)) => json!({
            "ok": false,
            "error": { "kind": e.kind.as_str(), "message": e.message, "method": method },
        })
        .to_string(),
        Err(_) => json!({
            "ok": false,
            "error": { "kind": ErrorKind::Internal.as_str(), "message": "internal panic", "method": method },
        })
        .to_string(),
    }
}

// Each route_* function owns one domain's method names. `route` tries them in
// turn and moves to the next as long as a function reports the method isn't
// one of its own (signaled by the UnknownMethod error its catch-all arm
// produces) — so every method is still handled by exactly one place, just
// grouped by domain instead of one 500+ line match.
const ROUTERS: &[fn(&str, &str) -> Outcome] = &[
    route_engine_lifecycle,
    route_addon_protocol,
    route_addon_resource,
    route_resource_plan,
    route_stream_policy,
    route_search_plan,
    route_player_policy,
    route_watchlist,
    route_offline,
    route_content_identity,
    route_calendar,
    route_external_sync_trakt,
    route_external_sync_simkl,
    route_external_sync_anilist,
    route_anime_detection,
    route_library_state,
    route_tmdb,
    route_intro_segments,
    route_core_contract,
];

fn route(method: &str, args_json: &str) -> Outcome {
    for router in ROUTERS {
        match router(method, args_json) {
            Err(CallError {
                kind: ErrorKind::UnknownMethod,
                ..
            }) => continue,
            result => return result,
        }
    }
    Err(fail(
        ErrorKind::UnknownMethod,
        format!("no such method `{method}`"),
    ))
}

fn route_engine_lifecycle(method: &str, args_json: &str) -> Outcome {
    match method {
        "engine.create" => Ok(json!(
            headless_engine::create_headless_engine(args_json) as i64
        )),
        "engine.snapshot" => result_json(
            headless_engine::headless_engine_snapshot_json(handle(args_json)?),
            method,
        ),
        "engine.dispatch" => {
            let args = object(args_json)?;
            result_json(
                headless_engine::headless_engine_dispatch_json(
                    field_u64(&args, "handle")?,
                    &field(&args, "action")?.to_string(),
                ),
                method,
            )
        }
        "engine.completeEffect" => {
            let args = object(args_json)?;
            result_json(
                headless_engine::headless_engine_complete_effect_json(
                    field_u64(&args, "handle")?,
                    &field(&args, "result")?.to_string(),
                ),
                method,
            )
        }
        "engine.destroy" => Ok(json!(headless_engine::destroy_headless_engine(handle(
            args_json
        )?))),

        // App state (parallel to headless engine, used by Android)
        "app.create" => Ok(json!(app_state::create_app_core_state(args_json) as i64)),
        "app.state" => result_json(app_state::app_core_state_json(handle(args_json)?), method),
        "app.dispatch" => {
            let args = object(args_json)?;
            result_json(
                app_state::app_core_dispatch_json(
                    field_u64(&args, "handle")?,
                    &field(&args, "action")?.to_string(),
                ),
                method,
            )
        }
        "app.destroy" => Ok(json!(app_state::destroy_app_core_state(handle(args_json)?))),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_addon_protocol(method: &str, args_json: &str) -> Outcome {
    match method {
        "identity" => Ok(Value::String(addon_protocol::identity(&arg_str(
            args_json, "url",
        )?))),
        "normalizeManifestUrl" => Ok(Value::String(addon_protocol::normalize_manifest_url(
            &arg_str(args_json, "url")?,
        ))),
        "manifestFetchPlan" => opt_json(addon_protocol::manifest_fetch_plan_json(&arg_str(
            args_json, "url",
        )?)),
        "parseManifest" => {
            let args = object(args_json)?;
            opt_json(addon_protocol::parse_manifest(
                field_str(&args, "body")?,
                field_str(&args, "transportUrl")?,
                "Unknown Addon",
            ))
        }
        // args_json IS the descriptor object
        "resolveManifestAssets" => {
            opt_json(addon_protocol::resolve_manifest_assets_json(args_json))
        }
        "mergeLiveManifest" => {
            let args = object(args_json)?;
            let live = args.get("live").and_then(Value::as_str).map(str::to_string);
            let name = args
                .get("unknownName")
                .and_then(Value::as_str)
                .unwrap_or("Unknown Addon");
            opt_json(addon_protocol::merge_live_manifest_json(
                field_str(&args, "descriptor")?,
                live.as_deref(),
                name,
            ))
        }
        "buildResourceUrl" => {
            let args = object(args_json)?;
            let extra = args
                .get("extraJson")
                .and_then(Value::as_str)
                .map(str::to_string);
            Ok(Value::String(addon_protocol::build_resource_url(
                field_str(&args, "transportUrl")?,
                field_str(&args, "resource")?,
                field_str(&args, "contentType")?,
                field_str(&args, "id")?,
                extra.as_deref(),
            )))
        }
        "supportsResource" => {
            let args = object(args_json)?;
            let content_type = args
                .get("contentType")
                .and_then(Value::as_str)
                .map(str::to_string);
            let id = args.get("id").and_then(Value::as_str).map(str::to_string);
            Ok(json!(addon_protocol::supports_resource(
                field_str(&args, "manifest")?,
                field_str(&args, "resource")?,
                content_type.as_deref(),
                id.as_deref(),
            )))
        }
        "catalogSupportsExtra" => {
            let args = object(args_json)?;
            Ok(json!(addon_protocol::catalog_supports_extra(
                field_str(&args, "catalog")?,
                field_str(&args, "extraName")?,
            )))
        }
        "catalogRequiresExtra" => {
            let args = object(args_json)?;
            Ok(json!(addon_protocol::catalog_requires_extra(
                field_str(&args, "catalog")?,
                field_str(&args, "extraName")?,
            )))
        }
        "catalogHasRequiredExtraExcept" => {
            let args = object(args_json)?;
            Ok(json!(addon_protocol::catalog_has_required_extra_except(
                field_str(&args, "catalog")?,
                field_str(&args, "allowedNames")?,
            )))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_addon_resource(method: &str, args_json: &str) -> Outcome {
    match method {
        "parseAddonResourceResult" => {
            let args = object(args_json)?;
            let body = args.get("body").and_then(Value::as_str).map(str::to_string);
            let status_code = field(&args, "statusCode")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "statusCode must be a number"))?
                as i32;
            into_json(addon_resource::parse_addon_resource_result_json(
                field_str(&args, "resource")?,
                field_str(&args, "url")?,
                status_code,
                body.as_deref(),
            ))
        }
        "normalizeAddonSubtitles" => {
            let args = object(args_json)?;
            into_json(addon_resource::normalize_addon_subtitles_json(
                field_str(&args, "subtitles")?,
                field_str(&args, "resourceUrl")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_resource_plan(method: &str, args_json: &str) -> Outcome {
    match method {
        // Repository / resource flow — args_json IS the request object
        "addonResourceRequestPlan" => {
            opt_json(repository_flow::addon_resource_request_plan_json(args_json))
        }
        "resourceFetchPlan" => opt_json(platform_plan::resource_fetch_plan_json(args_json)),
        "resourceParsePlan" => opt_json(platform_plan::resource_parse_plan_json(args_json)),

        // Platform plan — args_json IS the request object
        "playbackPreparePlan" => opt_json(platform_plan::playback_prepare_plan_json(args_json)),
        "libraryLocalStatePlan" => {
            opt_json(platform_plan::library_local_state_plan_json(args_json))
        }
        "preferencesSchema" => into_json(platform_plan::preferences_schema_json()),
        "applyPreferenceUpdate" => opt_json(platform_plan::apply_preference_update_json(args_json)),
        "addonCollectionMutationPlan" => opt_json(
            platform_plan::addon_collection_mutation_plan_json(args_json),
        ),
        "detailEpisodePlan" => opt_json(platform_plan::detail_episode_plan_json(args_json)),
        "resourceKindToResource" => {
            let args = object(args_json)?;
            Ok(Value::String(platform_plan::resource_kind_to_resource(
                field_str(&args, "kind")?,
                args.get("requestResource").and_then(Value::as_str),
                args.get("itemResource").and_then(Value::as_str),
            )))
        }
        "wrapAddonResourceResponse" => {
            let args = object(args_json)?;
            into_json(platform_plan::wrap_addon_resource_response(
                field_str(&args, "resource")?,
                field_str(&args, "payloadJson")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_stream_policy(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the stream/request JSON
        "streamPlaybackInfo" => opt_json(stream_policy::stream_playback_info_json(args_json)),
        "torrentRuntimeInfo" => opt_json(stream_policy::torrent_runtime_info_json(args_json)),
        "findPreferredSubtitleIndex" => {
            let args = object(args_json)?;
            let last = args
                .get("lastSubtitleLanguage")
                .and_then(Value::as_str)
                .map(str::to_string);
            let preferred = args
                .get("preferredSubtitleLanguage")
                .and_then(Value::as_str)
                .map(str::to_string);
            let secondary = args
                .get("secondarySubtitleLanguage")
                .and_then(Value::as_str)
                .map(str::to_string);
            Ok(json!(stream_policy::find_preferred_subtitle_index(
                field_str(&args, "tracks")?,
                last.as_deref(),
                preferred.as_deref(),
                secondary.as_deref(),
            )))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_search_plan(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object for single-arg methods
        "searchResultGrouping" => opt_json(search_plan::search_result_grouping_json(args_json)),
        "buildMetadataFeedOptions" => {
            opt_json(search_plan::build_metadata_feed_options_json(args_json))
        }
        "discoverCatalogOptions" => {
            let args = object(args_json)?;
            opt_json(search_plan::discover_catalog_options_json(
                field_str(&args, "addons")?,
                field_str(&args, "selectedType")?,
            ))
        }
        "discoverSortPlan" => opt_json(search_plan::discover_sort_plan_json(args_json)),
        "librarySortPlan" => opt_json(search_plan::library_sort_plan_json(args_json)),
        "detailSeriesLookupId" => Ok(Value::String(search_plan::detail_series_lookup_id(
            &arg_str(args_json, "id")?,
        ))),
        "detailSeasonLoadPlan" => opt_json(search_plan::detail_season_load_plan_json(args_json)),
        "resolveTransportUrl" => {
            let args = object(args_json)?;
            opt_json(search_plan::resolve_transport_url_json(
                field_str(&args, "sourceJson")?,
                field_str(&args, "addonsJson")?,
            ))
        }
        "resolveFeedOptionGenre" => {
            let args = object(args_json)?;
            opt_json(search_plan::resolve_feed_option_genre_json(
                field_str(&args, "feedOptionJson")?,
                field_str(&args, "addonsJson")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_player_policy(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object for single-arg methods
        "playerBackendSelection" => {
            opt_json(player_policy::player_backend_selection_json(args_json))
        }
        "playerBufferTargets" => opt_json(player_policy::player_buffer_targets_json(args_json)),
        "playerRetryPolicy" => opt_json(player_policy::player_retry_policy_json(args_json)),
        "playerSourceSidebarPlan" => {
            opt_json(player_policy::player_source_sidebar_plan_json(args_json))
        }
        "canPrefetchNextEpisode" => {
            let args = object(args_json)?;
            Ok(json!(player_policy::can_prefetch_next_episode_json(
                field_str(&args, "prefsJson")?,
                field_str(&args, "streamJson")?,
            )))
        }
        "selectNextEpisodeStream" => {
            let args = object(args_json)?;
            opt_json(player_policy::select_next_episode_stream_json(
                field_str(&args, "streamsJson")?,
                field_str(&args, "currentStreamJson")?,
                field_str(&args, "prefsJson")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_watchlist(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object
        "watchlistTogglePlan" => opt_json(watchlist_plan::watchlist_toggle_plan_json(args_json)),
        "playbackProgressMergePlan" => {
            opt_json(watchlist_plan::playback_progress_merge_plan_json(args_json))
        }
        "libraryApplyMarkWatched" => {
            let args = object(args_json)?;
            opt_json(watchlist_plan::library_apply_mark_watched_json(
                field_str(&args, "libJson")?,
                field_str(&args, "videoIdsJson")?,
            ))
        }
        "mergeProgressMeta" => {
            let args = object(args_json)?;
            into_json(watchlist_plan::merge_progress_meta_json(
                field_str(&args, "incomingMetaJson")?,
                field_str(&args, "existingMetaJson")?,
            ))
        }
        "importCollections" => opt_json(watchlist_plan::import_collections_json(args_json)),
        "exportCollections" => opt_json(watchlist_plan::export_collections_json(args_json)),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_offline(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object
        "offlineDownloadPlan" => opt_json(offline_download::offline_download_plan_json(args_json)),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_content_identity(method: &str, args_json: &str) -> Outcome {
    match method {
        "parseVideoId" => into_json(content_identity::parse_video_id_json(&arg_str(
            args_json, "id",
        )?)),
        "buildTraktIds" => opt_json(content_identity::build_trakt_ids_json(&arg_str(
            args_json, "id",
        )?)),
        "playbackIntroLookupContentId" => Ok(Value::String(
            content_identity::playback_intro_lookup_content_id(&arg_str(args_json, "id")?),
        )),
        "effectiveMetadataFeedSelection" => {
            let args = object(args_json)?;
            opt_json(content_identity::effective_metadata_feed_selection_json(
                field_str(&args, "selectedKeys")?,
                field_str(&args, "availableKeys")?,
            ))
        }
        "toggleMetadataFeedLimited" => {
            let args = object(args_json)?;
            let max_enabled = field(&args, "maxEnabled")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "maxEnabled must be a number"))?
                as i32;
            opt_json(content_identity::toggle_metadata_feed_limited_json(
                field_str(&args, "selectedKeys")?,
                field_str(&args, "availableKeys")?,
                field_str(&args, "key")?,
                max_enabled,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_calendar(method: &str, args_json: &str) -> Outcome {
    match method {
        "calendarItemsFromMeta" => {
            let args = object(args_json)?;
            opt_json(calendar_plan::calendar_items_from_meta_json(
                field_str(&args, "metaJson")?,
                field_str(&args, "monthPrefix")?,
            ))
        }
        "calendarItemMatchesMonth" => {
            let args = object(args_json)?;
            Ok(json!(calendar_plan::calendar_item_matches_month_json(
                field_str(&args, "itemJson")?,
                field_str(&args, "monthPrefix")?,
            )))
        }
        "nextUnairedEpisode" => {
            let args = object(args_json)?;
            let now_ms = field(&args, "nowMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "nowMs must be a number"))?;
            opt_json(calendar_plan::next_unaired_episode_json(
                field_str(&args, "videosJson")?,
                now_ms,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_external_sync_trakt(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the items array for single-array-arg methods
        "traktPlaybackItemsToLibrary" => opt_json(
            external_sync::trakt_playback_items_to_library_json(args_json),
        ),
        "traktWatchlistToItems" => {
            let args = object(args_json)?;
            opt_json(external_sync::trakt_watchlist_to_items_json(
                field_str(&args, "moviesJson")?,
                field_str(&args, "showsJson")?,
            ))
        }
        "stremioWatchlistToItems" => {
            opt_json(external_sync::stremio_watchlist_to_items_json(args_json))
        }
        "stremioWatchedToIds" => {
            opt_json(external_sync::stremio_watched_to_ids_json(args_json))
        }
        "traktWatchedToIds" => {
            let args = object(args_json)?;
            opt_json(external_sync::trakt_watched_to_ids_json(
                field_str(&args, "moviesJson")?,
                field_str(&args, "showsJson")?,
            ))
        }
        "mergeExternalWatchlist" => {
            let args = object(args_json)?;
            into_json(external_sync::merge_external_watchlist_json(
                field_str(&args, "localJson")?,
                field_str(&args, "externalJson")?,
            ))
        }
        "mergeExternalWatched" => {
            let args = object(args_json)?;
            into_json(external_sync::merge_external_watched_json(
                field_str(&args, "localJson")?,
                field_str(&args, "externalJson")?,
            ))
        }
        "mergeContinueWatchingLists" => {
            let args = object(args_json)?;
            opt_json(external_sync::merge_continue_watching_lists_json(
                field_str(&args, "localJson")?,
                field_str(&args, "externalJson")?,
                field_str(&args, "progressJson")?,
            ))
        }
        "traktScrobblePlan" => {
            let args = object(args_json)?;
            let season = args.get("season").and_then(Value::as_i64);
            let ep_number = args.get("epNumber").and_then(Value::as_i64);
            let time_pos = field(&args, "timePosSec")?
                .as_f64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "timePosSec must be a number"))?;
            let duration = field(&args, "durationSec")?
                .as_f64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "durationSec must be a number"))?;
            let ids_json = content_identity::build_trakt_ids_json(field_str(&args, "videoId")?)
                .ok_or_else(|| fail(ErrorKind::NotFound, "could not build trakt ids"))?;
            opt_json(player_scrobble::trakt_scrobble_plan_json(
                &ids_json,
                field(&args, "isEpisode")?
                    .as_bool()
                    .ok_or_else(|| fail(ErrorKind::InvalidArgs, "isEpisode must be bool"))?,
                season,
                ep_number,
                time_pos,
                duration,
            ))
        }
        "replaceExternalContinueWatching" => {
            let args = object(args_json)?;
            let provider = args.get("provider").and_then(Value::as_str);
            into_json(external_sync::replace_external_continue_watching_json(
                field_str(&args, "existingJson")?,
                provider,
                field_str(&args, "itemsJson")?,
            ))
        }
        "traktPlaybackItemsDedup" => {
            opt_json(external_sync::trakt_playback_items_dedup_json(args_json))
        }
        "traktMarkWatchedBody" => opt_json(external_sync::trakt_mark_watched_body_json(args_json)),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_external_sync_simkl(method: &str, args_json: &str) -> Outcome {
    match method {
        "simklWatchingToItems" => {
            let args = object(args_json)?;
            opt_json(external_sync::simkl_watching_to_items_json(
                field_str(&args, "showsJson")?,
                field_str(&args, "moviesJson")?,
            ))
        }
        "simklWatchlistToItems" => {
            let args = object(args_json)?;
            opt_json(external_sync::simkl_watchlist_to_items_json(
                field_str(&args, "showsJson")?,
                field_str(&args, "moviesJson")?,
            ))
        }
        "simklWatchedToIds" => {
            let args = object(args_json)?;
            opt_json(external_sync::simkl_watched_to_ids_json(
                field_str(&args, "showsJson")?,
                field_str(&args, "moviesJson")?,
            ))
        }
        "simklScrobbleBody" => {
            let args = object(args_json)?;
            let season = field(&args, "season")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "season must be a number"))?;
            let ep_number = field(&args, "epNumber")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "epNumber must be a number"))?;
            let time_pos = field(&args, "timePosSec")?
                .as_f64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "timePosSec must be a number"))?;
            let duration = field(&args, "durationSec")?
                .as_f64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "durationSec must be a number"))?;
            opt_json(player_scrobble::simkl_scrobble_body_json(
                field_str(&args, "idsJson")?,
                field(&args, "isEpisode")?
                    .as_bool()
                    .ok_or_else(|| fail(ErrorKind::InvalidArgs, "isEpisode must be bool"))?,
                season,
                ep_number,
                time_pos,
                duration,
            ))
        }
        "simklMatchEpisode" => {
            let args = object(args_json)?;
            opt_json(external_sync::simkl_match_episode_json(
                field_str(&args, "episodesJson")?,
                field_str(&args, "targetJson")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_external_sync_anilist(method: &str, args_json: &str) -> Outcome {
    match method {
        "anilistEntriesToSync" => {
            let args = object(args_json)?;
            let now_ms = field(&args, "nowMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "nowMs must be a number"))?;
            opt_json(external_sync::anilist_entries_to_sync_json(
                field_str(&args, "entriesJson")?,
                now_ms,
            ))
        }
        "mergeLibraryItemsById" => {
            let args = object(args_json)?;
            into_json(external_sync::merge_library_items_by_id_json(
                field_str(&args, "localJson")?,
                field_str(&args, "incomingJson")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_anime_detection(method: &str, args_json: &str) -> Outcome {
    match method {
        "detectAnimePlayback" => {
            let args = object(args_json)?;
            into_json(anime_detection::detect_anime_playback_json(
                field_str(&args, "metaJson")?,
                field_str(&args, "episodeJson")?,
                field_str(&args, "streamJson")?,
                field_str(&args, "addonsJson")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_library_state(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the items/item/doc JSON for single-arg methods
        "libraryContinueWatchingItems" => opt_json(
            library_state::library_continue_watching_items_json(args_json),
        ),
        "normalizeLibraryDocument" => {
            into_json(library_state::normalize_library_document_json(args_json))
        }
        "isUpNextContinueWatchingItem" => Ok(json!(
            library_state::is_up_next_continue_watching_item_json(args_json)
        )),
        "buildContinueWatchingFromProgress" => opt_json(
            library_state::build_continue_watching_from_progress_json(args_json),
        ),
        "rememberLastWatchedEpisodes" => {
            let args = object(args_json)?;
            into_json(library_state::remember_last_watched_episodes_json(
                field_str(&args, "libJson")?,
                field_str(&args, "watchedIdsJson")?,
            ))
        }
        "computeContinueWatchingBadges" => {
            let args = object(args_json)?;
            let now_ms = field(&args, "nowMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "nowMs must be a number"))?;
            opt_json(library_state::compute_continue_watching_badges_json(
                field_str(&args, "candidatesJson")?,
                field_str(&args, "videosBySeriesJson")?,
                field_str(&args, "lastWatchedJson")?,
                now_ms,
            ))
        }
        "resolveNextEpisode" => {
            let args = object(args_json)?;
            opt_json(library_state::resolve_next_episode_json(
                &field(&args, "videos")?.to_string(),
                field(&args, "currentSeason")?.as_i64().ok_or_else(|| {
                    fail(ErrorKind::InvalidArgs, "currentSeason must be a number")
                })?,
                field(&args, "currentEpisode")?.as_i64().ok_or_else(|| {
                    fail(ErrorKind::InvalidArgs, "currentEpisode must be a number")
                })?,
                field(&args, "nowMs")?
                    .as_i64()
                    .ok_or_else(|| fail(ErrorKind::InvalidArgs, "nowMs must be a number"))?,
                field(&args, "releasedOnly")?
                    .as_bool()
                    .ok_or_else(|| fail(ErrorKind::InvalidArgs, "releasedOnly must be bool"))?,
            ))
        }
        "formatEpisodeLine" => {
            let args = object(args_json)?;
            Ok(Value::String(library_state::format_episode_line_json(
                args.get("lastEpisodeName").and_then(Value::as_str),
                args.get("lastEpisodeSeason").and_then(Value::as_i64),
                args.get("lastEpisodeNumber").and_then(Value::as_i64),
                args.get("lastVideoId").and_then(Value::as_str),
            )))
        }
        "selectContinueWatchingArtwork" => {
            let args = object(args_json)?;
            Ok(json!(library_state::select_continue_watching_artwork_json(
                &field(&args, "item")?.to_string(),
                field_str(&args, "artworkPreference")?,
                field(&args, "isHorizontal")?
                    .as_bool()
                    .ok_or_else(|| fail(ErrorKind::InvalidArgs, "isHorizontal must be bool"))?,
            )))
        }
        "continueWatchingCardFields" => {
            let args = object(args_json)?;
            opt_json(library_state::continue_watching_card_fields_json(
                &field(&args, "items")?.to_string(),
                field_str(&args, "artworkPreference")?,
                field(&args, "isHorizontal")?
                    .as_bool()
                    .ok_or_else(|| fail(ErrorKind::InvalidArgs, "isHorizontal must be bool"))?,
            ))
        }
        "buildHomeCollectionShelves" => {
            let args = object(args_json)?;
            opt_json(home_ranking::build_home_collection_shelves_json(
                field_str(&args, "profileJson")?,
                field_str(&args, "addonsJson")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_tmdb(method: &str, args_json: &str) -> Outcome {
    match method {
        "tmdbContentType" => Ok(Value::String(
            tmdb_plan::tmdb_content_type(&arg_str(args_json, "contentType")?).to_string(),
        )),
        "tmdbLanguage" => Ok(Value::String(tmdb_plan::tmdb_language(&arg_str(
            args_json, "language",
        )?))),
        "tmdbImageUrl" => {
            let args = object(args_json)?;
            Ok(json!(tmdb_plan::tmdb_image_url(
                args.get("path").and_then(Value::as_str),
                field_str(&args, "size")?,
            )))
        }
        "tmdbMetaToMeta" => {
            let args = object(args_json)?;
            opt_json(tmdb_plan::tmdb_meta_to_meta_json(
                field_str(&args, "itemJson")?,
                field_str(&args, "requestedType")?,
                field_str(&args, "language")?,
            ))
        }
        // args_json IS the video/items JSON for single-arg methods
        "tmdbVideoToTrailer" => opt_json(tmdb_plan::tmdb_video_to_trailer_json(args_json)),
        "tmdbBulkMetas" => {
            let args = object(args_json)?;
            opt_json(tmdb_plan::tmdb_bulk_metas_to_metas_json(
                field_str(&args, "itemsJson")?,
                field_str(&args, "requestedType")?,
                field_str(&args, "language")?,
            ))
        }
        "tmdbBulkVideosToTrailers" => {
            opt_json(tmdb_plan::tmdb_bulk_videos_to_trailers_json(args_json))
        }
        "tmdbResolveIdHint" => {
            let (content_type, is_movie) =
                tmdb_plan::tmdb_resolve_id_hint(&arg_str(args_json, "contentId")?);
            Ok(json!([content_type, is_movie]))
        }
        "tmdbPeopleRequestPlan" => {
            let args = object(args_json)?;
            opt_json(tmdb_plan::tmdb_people_request_plan_json(
                field_str(&args, "metaJson")?,
                field_str(&args, "apiKey")?,
                field_str(&args, "language")?,
            ))
        }
        "tmdbCreditsUrlFromFind" => {
            let args = object(args_json)?;
            Ok(json!(tmdb_plan::tmdb_credits_url_from_find_json(
                field_str(&args, "findJson")?,
                field_str(&args, "metaJson")?,
                field_str(&args, "apiKey")?,
                field_str(&args, "language")?,
            )))
        }
        "tmdbPeopleImagesFromCredits" => {
            let args = object(args_json)?;
            opt_json(tmdb_plan::tmdb_people_images_from_credits_json(
                field_str(&args, "creditsJson")?,
                field_str(&args, "linksJson")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_intro_segments(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the data JSON for single-arg methods
        "parseIntroDbSegments" => opt_json(intro_segments::parse_intro_db_segments_json(args_json)),
        "parseAniskipResults" => opt_json(intro_segments::parse_aniskip_results_json(args_json)),
        "uniqueIntroSegments" => {
            let args = object(args_json)?;
            opt_json(intro_segments::unique_intro_segments_json(
                field_str(&args, "segmentsAJson")?,
                field_str(&args, "segmentsBJson")?,
            ))
        }
        "mergeIntroSegments" => opt_json(intro_segments::merge_intro_segments_json(args_json)),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_core_contract(method: &str, args_json: &str) -> Outcome {
    match method {
        "coreCapabilities" => into_json(core_contract::core_capabilities_json(
            object(args_json)
                .ok()
                .and_then(|o| o.get("portable").and_then(Value::as_bool))
                .unwrap_or(false),
        )),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn opt_json(value: Option<String>) -> Outcome {
    Ok(match value {
        Some(s) => serde_json::from_str(&s).map_err(|e| {
            fail(
                ErrorKind::Internal,
                format!("core produced invalid JSON: {e}"),
            )
        })?,
        None => Value::Null,
    })
}

fn object(args_json: &str) -> Result<Value, CallError> {
    let value: Value = serde_json::from_str(args_json).map_err(|e| {
        fail(
            ErrorKind::InvalidArgs,
            format!("args is not valid JSON: {e}"),
        )
    })?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(fail(ErrorKind::InvalidArgs, "args must be a JSON object"))
    }
}

fn arg_str(args_json: &str, name: &str) -> Result<String, CallError> {
    let args = object(args_json)?;
    Ok(field_str(&args, name)?.to_string())
}

fn field<'a>(args: &'a Value, name: &str) -> Result<&'a Value, CallError> {
    args.get(name)
        .ok_or_else(|| fail(ErrorKind::InvalidArgs, format!("missing field `{name}`")))
}

fn field_str<'a>(args: &'a Value, name: &str) -> Result<&'a str, CallError> {
    field(args, name)?.as_str().ok_or_else(|| {
        fail(
            ErrorKind::InvalidArgs,
            format!("field `{name}` must be a string"),
        )
    })
}

fn field_u64(args: &Value, name: &str) -> Result<u64, CallError> {
    field(args, name)?.as_u64().ok_or_else(|| {
        fail(
            ErrorKind::InvalidArgs,
            format!("field `{name}` must be a non-negative integer"),
        )
    })
}

fn handle(args_json: &str) -> Result<u64, CallError> {
    let value: Value = serde_json::from_str(args_json).map_err(|e| {
        fail(
            ErrorKind::InvalidArgs,
            format!("args is not valid JSON: {e}"),
        )
    })?;
    value
        .as_u64()
        .or_else(|| value.get("handle").and_then(Value::as_u64))
        .ok_or_else(|| {
            fail(
                ErrorKind::InvalidArgs,
                "expected a handle (number or { handle })",
            )
        })
}

fn result_json(value: Option<String>, method: &str) -> Outcome {
    match value {
        Some(s) => into_json(s),
        None => Err(fail(
            ErrorKind::NotFound,
            format!("`{method}` produced no result"),
        )),
    }
}

fn into_json(s: String) -> Outcome {
    serde_json::from_str(&s).map_err(|e| {
        fail(
            ErrorKind::Internal,
            format!("core produced invalid JSON: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn unknown_method_reports_kind_and_name() {
        let env = parse(&core_invoke("nope.doesNotExist", "{}"));
        assert_eq!(env["ok"], json!(false));
        assert_eq!(env["error"]["kind"], json!("unknown_method"));
        assert_eq!(env["error"]["method"], json!("nope.doesNotExist"));
    }

    #[test]
    fn invalid_args_distinguished_from_empty_result() {
        let bad_json = parse(&core_invoke("identity", "{ not json"));
        assert_eq!(bad_json["error"]["kind"], json!("invalid_args"));

        let missing_field = parse(&core_invoke("identity", "{}"));
        assert_eq!(missing_field["error"]["kind"], json!("invalid_args"));
    }

    #[test]
    fn stateless_helper_returns_ok_value() {
        let env = parse(&core_invoke("parseVideoId", r#"{"id":"tt123:1:2"}"#));
        assert_eq!(env["ok"], json!(true));
        assert_eq!(env["value"]["imdb"], json!("tt123"));
        assert_eq!(env["value"]["isEpisode"], json!(true));
    }

    #[test]
    fn new_sync_and_detection_methods_are_routed() {
        let detect = parse(&core_invoke(
            "detectAnimePlayback",
            r#"{"metaJson":"{\"genres\":[\"Anime\"]}","episodeJson":"null","streamJson":"null","addonsJson":"[]"}"#,
        ));
        assert_eq!(detect["ok"], json!(true));
        assert_eq!(detect["value"]["confidence"], json!(65));

        let sync = parse(&core_invoke(
            "anilistEntriesToSync",
            r#"{"entriesJson":"[]","nowMs":0}"#,
        ));
        assert_eq!(sync["ok"], json!(true));
        assert_eq!(sync["value"]["watchlist"], json!([]));

        let merged = parse(&core_invoke(
            "mergeLibraryItemsById",
            r#"{"localJson":"[]","incomingJson":"[{\"id\":\"a\"}]"}"#,
        ));
        assert_eq!(merged["value"][0]["id"], json!("a"));

        let plan = parse(&core_invoke(
            "tmdbPeopleRequestPlan",
            r#"{"metaJson":"{\"id\":\"tt123\",\"type\":\"movie\"}","apiKey":"k","language":"en"}"#,
        ));
        assert_eq!(
            plan["value"]["findUrl"],
            json!("https://api.themoviedb.org/3/find/tt123?api_key=k&language=en-US&external_source=imdb_id")
        );

        let images = parse(&core_invoke(
            "tmdbPeopleImagesFromCredits",
            r#"{"creditsJson":"{\"cast\":[{\"name\":\"Jane Doe\",\"profile_path\":\"/x.jpg\"}]}","linksJson":"[{\"name\":\"jane  doe\"}]"}"#,
        ));
        assert_eq!(
            images["value"]["jane  doe"],
            json!("https://image.tmdb.org/t/p/w185/x.jpg")
        );
    }

    #[test]
    fn engine_roundtrips_through_the_funnel() {
        let created = parse(&core_invoke("engine.create", "{}"));
        let h = created["value"].as_i64().unwrap();
        assert!(h > 0);

        let snap = parse(&core_invoke("engine.snapshot", &h.to_string()));
        assert_eq!(snap["ok"], json!(true));

        let destroyed = parse(&core_invoke("engine.destroy", &h.to_string()));
        assert_eq!(destroyed["ok"], json!(true));
        assert_eq!(destroyed["value"], json!(true));
    }
}
