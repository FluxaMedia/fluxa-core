use serde_json::{json, Value};

mod addon_resource_routes;
mod addon_support_routes;
mod engine_routes;
use addon_resource_routes::route_addon_resource;
use addon_support_routes::{route_addon_uptime, route_trailer_subtitles};
use engine_routes::route_engine_lifecycle;

#[cfg(feature = "native")]
use crate::dolby_vision_rpu;
use crate::{
    addon_protocol, addon_resource, addon_store, addon_uptime, anime_detection, app_state,
    calendar_plan, content_identity, core_contract, data_policy, desktop_playback, discovery_plan,
    external_sync, headless_adapter_plan, headless_engine, home_ranking, intro_segments,
    library_state, nuvio_sync, offline_download, platform_plan, player_flow, player_policy,
    player_scrobble, plugins, profile_avatar_pack, profile_contract, profile_prefs,
    repository_flow, search_plan, stream_policy, tmdb_plan, trailer_subtitles, watchlist_plan,
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
    route_addon_uptime,
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
    route_nuvio_sync,
    route_tmdb,
    route_intro_segments,
    route_core_contract,
    route_plugins,
    route_addon_store,
    route_profile_avatar_pack,
    route_profile_contract,
    route_profile_prefs,
    route_headless_adapter_plan,
    route_discovery_plan,
    route_data_policy,
    #[cfg(feature = "native")]
    route_dolby_vision_rpu,
    route_player_flow,
    route_player_scrobble,
    route_trailer_subtitles,
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
        "baseUrl" => Ok(Value::String(addon_protocol::base_url(&arg_str(
            args_json, "url",
        )?))),
        "preferHttpsAssetUrl" => Ok(json!(addon_protocol::prefer_https_asset_url(&arg_str(
            args_json, "url",
        )?))),
        "manifestCandidates" => Ok(json!(addon_protocol::manifest_candidates(&arg_str(
            args_json, "url",
        )?))),
        "parseManifest" => {
            let args = object(args_json)?;
            opt_json(addon_protocol::parse_manifest(
                field_str(&args, "body")?,
                field_str(&args, "transportUrl")?,
                field_str(&args, "unknownName")?,
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
        "normalizeAddonDescriptor" => opt_json(addon_protocol::normalize_addon_descriptor_json(
            &arg_str(args_json, "addonJson")?,
        )),
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
        // args_json IS the links array
        "classifyMetaLinks" => opt_json(addon_protocol::classify_meta_links_json(args_json)),

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
        "repositoryMetaDetailPlan" => {
            opt_json(repository_flow::repository_meta_detail_plan_json(args_json))
        }
        "manifestFetchDecision" => {
            opt_json(repository_flow::manifest_fetch_decision_json(args_json))
        }
        "repositorySeasonVideos" => {
            let args = object(args_json)?;
            let season_number = field(&args, "seasonNumber")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "seasonNumber must be a number"))?
                as i32;
            into_json(repository_flow::repository_season_videos_json(
                field_str(&args, "metaDetailJson")?,
                season_number,
            ))
        }
        "addonStreamsWithProvider" => {
            let args = object(args_json)?;
            into_json(repository_flow::addon_streams_with_provider_json(
                field_str(&args, "streamsJson")?,
                field_str(&args, "addonName")?,
            ))
        }
        "resourceFetchPlan" => opt_json(platform_plan::resource_fetch_plan_json(args_json)),
        "resourceFetchExecutionPolicy" => opt_json(
            platform_plan::resource_fetch_execution_policy_json(args_json),
        ),
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
        "seasonWatchedPlan" => opt_json(platform_plan::season_watched_plan_json(args_json)),
        "markSeasonsActionPlan" => {
            opt_json(platform_plan::mark_seasons_action_plan_json(args_json))
        }
        "resourceKindToResource" => {
            let args = object(args_json)?;
            Ok(Value::String(platform_plan::resource_kind_to_resource(
                field_str(&args, "kind")?,
                args.get("requestResource").and_then(Value::as_str),
                args.get("itemResource").and_then(Value::as_str),
            )))
        }
        "parseAndPlanAddonResource" => {
            let args = object(args_json)?;
            let body = args.get("body").and_then(Value::as_str).map(str::to_string);
            let status_code = field(&args, "statusCode")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "statusCode must be a number"))?
                as i32;
            let addon_name = args
                .get("addonName")
                .and_then(Value::as_str)
                .map(str::to_string);
            let season = args.get("season").and_then(Value::as_i64);
            into_json(platform_plan::parse_and_plan_addon_resource_json(
                field_str(&args, "resource")?,
                field_str(&args, "url")?,
                status_code,
                body.as_deref(),
                field_str(&args, "kind")?,
                addon_name.as_deref(),
                season,
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
        "torrentStatusInfo" => opt_json(stream_policy::torrent_status_info_json(args_json)),
        "torrentReadyBudget" => into_json(stream_policy::torrent_ready_budget_json()),
        "streamRequestHeaders" => opt_json(stream_policy::stream_request_headers_json(&arg_str(
            args_json,
            "headersJson",
        )?)),
        "streamRequestReferer" => opt_json(stream_policy::stream_request_referer(&arg_str(
            args_json, "url",
        )?)),
        "selectStreamIndex" => {
            let args = object(args_json)?;
            let saved_url = args.get("savedUrl").and_then(Value::as_str);
            let saved_title = args.get("savedTitle").and_then(Value::as_str);
            let regex_pattern = args.get("regexPattern").and_then(Value::as_str);
            let preferred_binge_group = args.get("preferredBingeGroup").and_then(Value::as_str);
            Ok(json!(stream_policy::select_stream_index(
                field_str(&args, "streamsJson")?,
                field_str(&args, "currentVideoId")?,
                field(&args, "initialStreamIndex")?
                    .as_i64()
                    .ok_or_else(|| fail(
                        ErrorKind::InvalidArgs,
                        "initialStreamIndex must be a number"
                    ))? as i32,
                saved_url,
                saved_title,
                field_str(&args, "sourceSelectionMode")?.into(),
                regex_pattern,
                preferred_binge_group,
            )))
        }
        "playerTrackState" => opt_json(stream_policy::player_track_state_json(args_json)),
        "resolvePreferredAudioLanguage" => {
            let args = object(args_json)?;
            let last = args.get("lastAudioLanguage").and_then(Value::as_str);
            let preferred = args.get("preferredAudioLanguage").and_then(Value::as_str);
            let original = args.get("originalLanguage").and_then(Value::as_str);
            Ok(Value::String(
                stream_policy::resolve_preferred_audio_language(last, preferred, original),
            ))
        }
        "subtitleLanguageMatches" => {
            let args = object(args_json)?;
            let language = args.get("language").and_then(Value::as_str);
            Ok(json!(stream_policy::subtitle_language_matches(
                field_str(&args, "label")?,
                language,
                field_str(&args, "preferredLanguage")?,
            )))
        }
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
        "searchSuggestionsPlan" => opt_json(search_plan::search_suggestions_plan_json(args_json)),
        "searchScreenPlan" => opt_json(search_plan::search_screen_plan_json(args_json)),
        "mergeDiscoverPages" => opt_json(search_plan::merge_discover_pages_json(args_json)),
        "recentSearchesPlan" => opt_json(search_plan::recent_searches_plan_json(args_json)),
        // args_json IS the sources array
        "mergeSearchSources" => opt_json(search_plan::merge_search_sources_json(args_json)),
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
        "discoverContentTypes" => opt_json(search_plan::discover_content_types_json(args_json)),
        "discoverSelectionPlan" => opt_json(search_plan::discover_selection_plan_json(args_json)),
        "librarySortPlan" => opt_json(search_plan::library_sort_plan_json(args_json)),
        "discoverSortPlan" => opt_json(search_plan::discover_sort_plan_json(args_json)),
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
        "dvProxyPlan" => opt_json(player_policy::dv_proxy_plan_json(args_json)),
        "torrentFallbackFilePolicy" => {
            opt_json(player_policy::torrent_fallback_file_policy_json(args_json))
        }
        // args_json IS the request object for single-arg methods
        "playerBackendSelection" => {
            opt_json(player_policy::player_backend_selection_json(args_json))
        }
        "playerBufferTargets" => opt_json(player_policy::player_buffer_targets_json(args_json)),
        "playerRetryPolicy" => opt_json(player_policy::player_retry_policy_json(args_json)),
        "nextRetrySourcePlan" => opt_json(player_policy::next_retry_source_plan_json(args_json)),
        "playbackClosePlan" => opt_json(player_policy::playback_close_plan_json(args_json)),
        "playbackPreferencesPlan" => {
            opt_json(player_policy::playback_preferences_plan_json(args_json))
        }
        "streamShellPlan" => opt_json(player_policy::stream_shell_plan_json(args_json)),
        "orderStreamsPlan" => opt_json(player_policy::order_streams_plan_json(args_json)),
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
                field_str(&args, "nextVideoId")?,
            ))
        }
        "chapterSkipSegments" => {
            let args = object(args_json)?;
            into_json(desktop_playback::chapter_skip_segments_json(field_str(
                &args,
                "chaptersJson",
            )?))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_watchlist(method: &str, args_json: &str) -> Outcome {
    match method {
        "remoteCollectionRequestPlan" => opt_json(
            watchlist_plan::remote_collection_request_plan_json(args_json),
        ),
        "remoteCollectionResponsePlan" => opt_json(
            watchlist_plan::remote_collection_response_plan_json(args_json),
        ),
        // args_json IS the request object
        "watchlistTogglePlan" => opt_json(watchlist_plan::watchlist_toggle_plan_json(args_json)),
        "libraryCommandPlan" => opt_json(watchlist_plan::library_command_plan_json(args_json)),
        "playbackProgressMergePlan" => {
            opt_json(watchlist_plan::playback_progress_merge_plan_json(args_json))
        }
        "playbackProgressWritePlan" => {
            opt_json(watchlist_plan::playback_progress_write_plan_json(args_json))
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
        "airDateRefreshCandidates" => {
            opt_json(watchlist_plan::air_date_refresh_candidates_json(args_json))
        }
        "airDateRefreshPlan" => opt_json(watchlist_plan::air_date_refresh_plan_json(args_json)),
        "applyAirDateUpdates" => opt_json(watchlist_plan::apply_air_date_updates_json(args_json)),
        "libraryViewPlan" => opt_json(watchlist_plan::library_view_plan_json(args_json)),
        "collectionMergePlan" => opt_json(watchlist_plan::collection_merge_plan_json(args_json)),
        "collectionFolderItemsPlan" => {
            opt_json(watchlist_plan::collection_folder_items_plan_json(args_json))
        }
        "importCollections" => opt_json(watchlist_plan::import_collections_json(args_json)),
        "exportCollections" => opt_json(watchlist_plan::export_collections_json(args_json)),
        "libraryExternalMergePlan" => {
            opt_json(watchlist_plan::library_external_merge_plan_json(args_json))
        }
        "libraryCollectionImportValidation" => opt_json(
            watchlist_plan::library_collection_import_validation_json(args_json),
        ),
        "libraryOfflineGrouping" => {
            opt_json(watchlist_plan::library_offline_grouping_json(args_json))
        }

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
        "contentImdbId" => Ok(json!(content_identity::imdb_id(&arg_str(args_json, "id")?))),
        "contentBaseId" => Ok(Value::String(content_identity::base_content_id(&arg_str(
            args_json, "id",
        )?))),
        "normalizeSeriesLookupId" => Ok(Value::String(
            content_identity::normalize_series_lookup_id(&arg_str(args_json, "id")?),
        )),
        "isTmdbLikeContentId" => Ok(json!(content_identity::is_tmdb_like_content_id(&arg_str(
            args_json, "id"
        )?))),
        "tmdbNumericId" => Ok(json!(content_identity::tmdb_numeric_id(&arg_str(
            args_json, "id"
        )?))),
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
        "streamRequestIds" => {
            let args = object(args_json)?;
            let detail_id = args.get("detailId").and_then(Value::as_str);
            let current_series_lookup_id =
                args.get("currentSeriesLookupId").and_then(Value::as_str);
            let canonical_base_id = args.get("canonicalBaseId").and_then(Value::as_str);
            Ok(json!(content_identity::stream_request_ids(
                field_str(&args, "contentType")?,
                field_str(&args, "id")?,
                detail_id,
                current_series_lookup_id,
                canonical_base_id,
            )))
        }
        "episodeTextMatches" => {
            let args = object(args_json)?;
            let season = field(&args, "season")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "season must be a number"))?
                as i32;
            let episode = field(&args, "episode")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "episode must be a number"))?
                as i32;
            Ok(json!(content_identity::text_matches_episode(
                field_str(&args, "text")?,
                season,
                episode,
            )))
        }
        "streamMatchesEpisode" => {
            let args = object(args_json)?;
            let fields = [
                args.get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                args.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                args.get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                args.get("filename")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                args.get("effectiveFilename")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ];
            Ok(json!(content_identity::stream_matches_episode(
                field_str(&args, "videoId")?,
                &fields,
            )))
        }
        "contentTraktKeysBatch" => opt_json(content_identity::content_trakt_keys_batch(&arg_str(
            args_json,
            "metasJson",
        )?)),
        "contentWatchedKeysBatch" => opt_json(content_identity::content_watched_keys_batch(
            &arg_str(args_json, "metasJson")?,
        )),
        "contentMergeKeys" => opt_json(content_identity::content_keys_json(
            &arg_str(args_json, "metaJson")?,
            false,
        )),
        "episodeFilenameCandidate" => {
            let args = object(args_json)?;
            opt_json(content_identity::episode_filename_candidate(
                field_str(&args, "streamJson")?,
                field_str(&args, "videoId")?,
            ))
        }
        "streamDiscoveryCacheKey" => {
            opt_str(content_identity::stream_discovery_cache_key(args_json))
        }
        "discoverCatalogCacheKey" => {
            opt_str(content_identity::discover_catalog_cache_key(args_json))
        }
        "stableFeedPart" => Ok(Value::String(content_identity::stable_feed_part(&arg_str(
            args_json, "value",
        )?))),
        "normalizeContentType" => Ok(json!(content_identity::normalize_content_type(&arg_str(
            args_json, "value",
        )?))),
        "parseExtraArgs" => opt_json(content_identity::parse_extra_args_json(&arg_str(
            args_json, "extra",
        )?)),
        "providerSearchTerms" => Ok(json!(content_identity::provider_search_terms(&arg_str(
            args_json, "provider",
        )?))),
        "filterDiscoverResults" => {
            let args = object(args_json)?;
            let year = args.get("year").and_then(Value::as_str);
            let rating = args.get("rating").and_then(Value::as_f64).map(|v| v as f32);
            let region = args.get("region").and_then(Value::as_str);
            opt_json(content_identity::filter_discover_results_json(
                field_str(&args, "itemsJson")?,
                year,
                rating,
                region,
            ))
        }
        "mergeContinueWatchingDuplicates" => {
            opt_json(content_identity::merge_continue_watching_duplicates_json(
                &arg_str(args_json, "itemsJson")?,
            ))
        }
        "directPlaybackPlan" => {
            let args = object(args_json)?;
            let detail_json = args.get("detailJson").and_then(Value::as_str);
            opt_json(content_identity::direct_playback_plan_json(
                field_str(&args, "metaJson")?,
                detail_json,
                field_str(&args, "todayIso")?,
            ))
        }
        "streamDiscoveryEpisodeContext" => {
            let args = object(args_json)?;
            let detail_json = args.get("detailJson").and_then(Value::as_str);
            opt_json(content_identity::stream_discovery_episode_context_json(
                field_str(&args, "contentType")?,
                field_str(&args, "requestId")?,
                detail_json,
                field_str(&args, "seasonEpisodesJson")?,
            ))
        }
        "parseEpisodeLocator" => {
            let raw = arg_str(args_json, "input")?;
            match content_identity::parse_episode_locator(&raw) {
                Some((base_id, season, episode)) => Ok(json!({
                    "baseId": base_id,
                    "season": season,
                    "episode": episode
                })),
                None => Ok(Value::Null),
            }
        }
        "playbackStreamRequestIds" => {
            let args = object(args_json)?;
            let detail_id = args.get("detailId").and_then(Value::as_str);
            opt_json(content_identity::playback_stream_request_ids_json(
                field_str(&args, "contentType")?,
                field_str(&args, "id")?,
                detail_id,
            ))
        }
        "toggleMetadataFeed" => {
            let args = object(args_json)?;
            opt_json(content_identity::toggle_metadata_feed_json(
                field_str(&args, "selectedKeys")?,
                field_str(&args, "availableKeys")?,
                field_str(&args, "key")?,
            ))
        }
        "setMetadataFeedGroupEnabled" => {
            let args = object(args_json)?;
            let enabled = field(&args, "enabled")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "enabled must be a bool"))?;
            opt_json(content_identity::set_metadata_feed_group_enabled_json(
                field_str(&args, "selectedKeys")?,
                field_str(&args, "availableKeys")?,
                field_str(&args, "groupKeys")?,
                enabled,
            ))
        }
        "orderedMetadataFeedKeys" => {
            let args = object(args_json)?;
            opt_json(content_identity::ordered_metadata_feed_keys(
                field_str(&args, "optionKeys")?,
                field_str(&args, "order")?,
            ))
        }
        "moveMetadataFeedOrder" => {
            let args = object(args_json)?;
            let delta = field(&args, "delta")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "delta must be a number"))?
                as i32;
            opt_json(content_identity::move_metadata_feed_order_json(
                field_str(&args, "optionKeys")?,
                field_str(&args, "currentOrder")?,
                field_str(&args, "key")?,
                delta,
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
        "calendarCandidatePlan" => opt_json(calendar_plan::calendar_candidate_plan_json(args_json)),
        "calendarVisibilityPlan" => {
            opt_json(calendar_plan::calendar_visibility_plan_json(args_json))
        }
        "calendarReleaseRows" => opt_json(calendar_plan::calendar_release_rows_json(args_json)),
        "calendarContentPlan" => opt_json(calendar_plan::calendar_content_plan_json(args_json)),
        "desktopCalendarReadPlan" => {
            opt_json(calendar_plan::desktop_calendar_read_plan_json(args_json))
        }
        "calendarSeasonCandidates" => {
            opt_json(calendar_plan::calendar_season_candidates_json(args_json))
        }
        "calendarWidgetRows" => opt_json(calendar_plan::calendar_widget_rows_json(args_json)),
        "calendarNotificationContent" => {
            opt_json(calendar_plan::calendar_notification_content_json(args_json))
        }
        "calendarReleaseDetection" => {
            opt_json(calendar_plan::calendar_release_detection_json(args_json))
        }
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
        "partitionThisWeek" => {
            let args = object(args_json)?;
            let now_ms = field(&args, "nowMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "nowMs must be a number"))?;
            let keep_scheduled = field(&args, "keepScheduled")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "keepScheduled must be bool"))?;
            opt_json(calendar_plan::partition_this_week_json(
                field_str(&args, "itemsJson")?,
                now_ms,
                keep_scheduled,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

mod external_sync_routes;

use external_sync_routes::{
    route_external_sync_anilist, route_external_sync_simkl, route_external_sync_trakt,
};

fn route_anime_detection(method: &str, args_json: &str) -> Outcome {
    match method {
        "detectAnimePlayback" => {
            let args = object(args_json)?;
            let empty: Vec<Value> = Vec::new();
            let addons = args
                .get("addons")
                .and_then(Value::as_array)
                .unwrap_or(&empty);
            Ok(anime_detection::detect_anime_playback(
                args.get("meta").unwrap_or(&Value::Null),
                args.get("episode").unwrap_or(&Value::Null),
                args.get("stream").unwrap_or(&Value::Null),
                addons,
            ))
        }
        // args_json IS the meta object
        "shouldAttemptAnimeTracking" => Ok(json!(anime_detection::should_attempt_anime_tracking(
            &object(args_json)?
        ))),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

mod library_routes;

use library_routes::route_library_state;

fn route_nuvio_sync(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object
        "nuvioBuildLocalProfiles" => opt_json(nuvio_sync::build_local_profiles_json(args_json)),
        "nuvioLibraryToWatchlist" => opt_json(nuvio_sync::library_to_watchlist_json(args_json)),
        "nuvioProgressMetaNeeds" => opt_json(nuvio_sync::progress_meta_needs_json(args_json)),
        "nuvioImportMergePlan" => opt_json(nuvio_sync::import_merge_plan_json(args_json)),
        "nuvioExportPushPlan" => opt_json(nuvio_sync::export_push_plan_json(args_json)),
        "nuvioLibraryMutationPlan" => opt_json(nuvio_sync::library_mutation_plan_json(args_json)),
        "nuvioMapCollections" => opt_json(nuvio_sync::map_collections_json(args_json)),
        "nuvioSortAddonsByPriority" => {
            opt_json(nuvio_sync::sort_addons_by_priority_json(args_json))
        }
        "nuvioAddonState" => opt_json(nuvio_sync::addon_state_json(args_json)),
        "nuvioAddonReconciliationPlan" => {
            opt_json(nuvio_sync::addon_reconciliation_plan_json(args_json))
        }
        "nuvioLibraryItemRequest" => opt_json(nuvio_sync::library_item_request_json(args_json)),
        "nuvioWatchedItemsRequest" => opt_json(nuvio_sync::watched_items_request_json(args_json)),
        "nuvioPlaybackProgressRequest" => {
            opt_json(nuvio_sync::playback_progress_request_json(args_json))
        }
        "nuvioCollectionRequest" => opt_json(nuvio_sync::collection_request_json(args_json)),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("`{method}` is not a nuvio-sync method"),
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
            Ok(tmdb_plan::tmdb_people_request_plan(
                field(&args, "meta")?,
                field_str(&args, "apiKey")?,
                field_str(&args, "language")?,
            ))
        }
        "tmdbCreditsUrlFromFind" => {
            let args = object(args_json)?;
            Ok(json!(tmdb_plan::tmdb_credits_url_from_find(
                field(&args, "find")?,
                field(&args, "meta")?,
                field_str(&args, "apiKey")?,
                field_str(&args, "language")?,
            )))
        }
        "tmdbBuiltinManifest" => Ok(Value::String(tmdb_plan::tmdb_builtin_manifest_json())),
        "tmdbBuiltinCatalogUrl" => {
            let args = object(args_json)?;
            Ok(Value::String(tmdb_plan::tmdb_builtin_catalog_url(
                field_str(&args, "contentType")?,
                field(&args, "extra")?,
                field_str(&args, "apiKey")?,
                field_str(&args, "language")?,
            )))
        }
        "tmdbFullMetaToMeta" => {
            let args = object(args_json)?;
            opt_json(tmdb_plan::tmdb_full_meta_to_meta_json(
                field_str(&args, "detailsJson")?,
                field_str(&args, "creditsJson")?,
                field_str(&args, "imagesJson")?,
                field_str(&args, "externalIdsJson")?,
                field_str(&args, "requestedType")?,
                field_str(&args, "language")?,
            ))
        }
        "tmdbEpisodesToVideos" => {
            let args = object(args_json)?;
            opt_json(tmdb_plan::tmdb_episodes_to_videos_json(
                field_str(&args, "seasonJson")?,
                field_str(&args, "seriesId")?,
            ))
        }
        "tmdbPeopleImagesFromCredits" => {
            let args = object(args_json)?;
            let empty: Vec<Value> = Vec::new();
            let links = args
                .get("links")
                .and_then(Value::as_array)
                .unwrap_or(&empty);
            Ok(tmdb_plan::tmdb_people_images_from_credits(
                field(&args, "credits")?,
                links,
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
        "anilistMalId" => opt_json(intro_segments::anilist_mal_id_json(args_json)),
        "parseAniskipResults" => opt_json(intro_segments::parse_aniskip_results_json(args_json)),
        "parseAnimeSkipResults" => {
            opt_json(intro_segments::parse_anime_skip_results_json(args_json))
        }
        "uniqueIntroSegments" => {
            let args = object(args_json)?;
            opt_json(intro_segments::unique_intro_segments_json(
                field_str(&args, "segmentsAJson")?,
                field_str(&args, "segmentsBJson")?,
            ))
        }
        "mergeIntroSegments" => opt_json(intro_segments::merge_intro_segments_json(args_json)),
        "matchAnimeSkipEpisodeId" => {
            let args = object(args_json)?;
            let season = field(&args, "season")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "season must be a number"))?;
            let episode = field(&args, "episode")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "episode must be a number"))?;
            opt_json(
                intro_segments::match_anime_skip_episode_id(
                    field_str(&args, "episodesJson")?,
                    season,
                    episode,
                )
                .and_then(|id| serde_json::to_string(&id).ok()),
            )
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_plugins(method: &str, args_json: &str) -> Outcome {
    match method {
        "pluginManifestParse" => {
            let normalized = plugins::parse_plugin_manifest_json(args_json)
                .map_err(|message| fail(ErrorKind::InvalidArgs, message))?;
            into_json(normalized)
        }
        "pluginExecutionPlan" => opt_json(plugins::plugin_execution_plan_json(args_json)),
        "pluginStreamResultsParse" => {
            into_json(plugins::parse_plugin_stream_results_json(args_json))
        }
        "pluginStreamResultsToStreams" => {
            into_json(plugins::plugin_stream_results_to_streams_json(args_json))
        }

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

fn route_addon_store(method: &str, args_json: &str) -> Outcome {
    match method {
        "addonStoreInputType" => Ok(Value::String(
            addon_store::addon_store_input_type(&arg_str(args_json, "input")?).to_string(),
        )),
        "normalizeCloudstreamRepoUrl" => Ok(Value::String(
            addon_store::normalize_cloudstream_repo_url(&arg_str(args_json, "url")?),
        )),
        "normalizePluginRepositoryUrl" => Ok(Value::String(
            addon_store::normalize_plugin_repository_url(&arg_str(args_json, "url")?),
        )),
        "isSecureRemoteUrl" => Ok(json!(addon_store::is_secure_remote_url(&arg_str(
            args_json, "url",
        )?))),
        "samePluginRepositoryUrl" => {
            let args = object(args_json)?;
            Ok(json!(addon_store::same_plugin_repository_url(
                field_str(&args, "left")?,
                field_str(&args, "right")?,
            )))
        }
        // args_json IS the profile object
        "profileLocalAddonsKey" => opt_str(addon_store::profile_local_addons_key_json(args_json)),
        "addonProfileMutationPlan" => {
            opt_json(addon_store::addon_profile_mutation_plan_json(args_json))
        }
        "sanitizeProfile" => {
            let args = object(args_json)?;
            let merge_mirrored_addons = field(&args, "mergeMirroredAddons")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "mergeMirroredAddons must be bool"))?;
            opt_json(addon_store::sanitize_profile_json(
                field_str(&args, "profile")?,
                field_str(&args, "mirroredAddons")?,
                merge_mirrored_addons,
            ))
        }
        // args_json IS the request object
        "addonStoreSearchPolicy" => {
            opt_json(addon_store::addon_store_search_policy_json(args_json))
        }
        "extractAddonManifestUrl" => opt_json(addon_store::extract_addon_manifest_url(&arg_str(
            args_json, "text",
        )?)),
        "filterEnabledAddons" => opt_json(addon_store::filter_enabled_addons_json(args_json)),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_profile_avatar_pack(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object for all of these. The platform owns
        // the HTTP calls between plans; this crate only validates and maps the
        // GitHub responses into the stable UI contract.
        "profileAvatarPackRepositoryPlan" => {
            opt_json(profile_avatar_pack::profile_avatar_pack_repository_plan_json(args_json))
        }
        "profileAvatarPackDiscoveryPlan" => {
            opt_json(profile_avatar_pack::profile_avatar_pack_discovery_plan_json(args_json))
        }
        "profileAvatarPackCatalog" => opt_json(
            profile_avatar_pack::profile_avatar_pack_catalog_json(args_json),
        ),
        "profileAvatarPackParse" => {
            opt_json(profile_avatar_pack::profile_avatar_pack_json(args_json))
        }
        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_profile_contract(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object for all of these
        "activeProfilePlan" => opt_json(profile_contract::active_profile_plan_json(args_json)),
        "tokenMergePlan" => opt_json(profile_contract::token_merge_plan_json(args_json)),
        "profileDefaultSeed" => opt_json(profile_contract::profile_default_seed_json(args_json)),
        "profileSettingsMigrationPlan" => opt_json(
            profile_contract::profile_settings_migration_plan_json(args_json),
        ),
        "profileAvatarDefault" => {
            opt_json(profile_contract::profile_avatar_default_json(args_json))
        }
        "profileMutationPlan" => opt_json(profile_contract::profile_mutation_plan_json(args_json)),
        "createProfilePlan" => opt_json(profile_contract::create_profile_plan_json(args_json)),
        "profilePinHash" => Ok(Value::String(profile_contract::profile_pin_hash(&arg_str(
            args_json, "pin",
        )?))),
        "profilePinMatches" => {
            let args = object(args_json)?;
            Ok(json!(profile_contract::profile_pin_matches(
                field_str(&args, "profileJson")?,
                field_str(&args, "pin")?
            )))
        }
        "profileConnectionState" => {
            let args = object(args_json)?;
            into_json(profile_contract::profile_connection_state_json(
                field_str(&args, "profileJson")?,
                field(&args, "nowEpochSeconds")?.as_i64().unwrap_or(0),
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_profile_prefs(method: &str, args_json: &str) -> Outcome {
    match method {
        "safePlayerBufferCacheMb" => {
            let args = object(args_json)?;
            let value = args.get("value").and_then(Value::as_i64).map(|v| v as i32);
            Ok(json!(profile_prefs::safe_player_buffer_cache_mb(value)))
        }
        "safeDolbyVisionFallbackMode" => {
            let args = object(args_json)?;
            let mode = args.get("mode").and_then(Value::as_str);
            let legacy_dv7_fallback = args.get("legacyDv7Fallback").and_then(Value::as_bool);
            let legacy_dv7_to_dv8_fallback =
                args.get("legacyDv7ToDv8Fallback").and_then(Value::as_bool);
            Ok(Value::String(
                profile_prefs::safe_dolby_vision_fallback_mode(
                    mode,
                    legacy_dv7_fallback,
                    legacy_dv7_to_dv8_fallback,
                )
                .to_string(),
            ))
        }
        "safeStreamSourceSelectionMode" => {
            let args = object(args_json)?;
            let mode = args.get("mode").and_then(Value::as_str);
            Ok(Value::String(
                profile_prefs::safe_stream_source_selection_mode(mode).to_string(),
            ))
        }
        // args_json IS the profile object
        "profileSafePrefs" => opt_json(profile_prefs::profile_safe_prefs_json(args_json)),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_headless_adapter_plan(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object
        "providerAvailabilityPlan" => opt_json(
            headless_adapter_plan::provider_availability_plan_json(args_json),
        ),
        "detailStreamResultPlan" => opt_json(
            headless_adapter_plan::detail_stream_result_plan_json(args_json),
        ),
        "prefetchDetailStreamsPlan" => opt_json(
            headless_adapter_plan::prefetch_detail_streams_plan_json(args_json),
        ),
        "directPlaybackPolicy" => into_json(headless_adapter_plan::direct_playback_policy_json()),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_discovery_plan(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object
        "streamDiscoveryPlan" => opt_json(discovery_plan::stream_discovery_plan_json(args_json)),
        "streamDiscoveryExecutionPolicy" => opt_json(
            discovery_plan::stream_discovery_execution_policy_json(args_json),
        ),
        "streamDiscoveryCachePrefix" => {
            let args = object(args_json)?;
            Ok(Value::String(
                discovery_plan::stream_discovery_cache_prefix(
                    field_str(&args, "contentType")?,
                    field_str(&args, "id")?,
                    field_str(&args, "language")?,
                ),
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_data_policy(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object for all of these
        "cacheEntryPolicy" => opt_json(data_policy::cache_entry_policy_json(args_json)),
        "cacheTrimPolicy" => opt_json(data_policy::cache_trim_policy_json(args_json)),
        "dataFailurePolicy" => opt_json(data_policy::data_failure_policy_json(args_json)),

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

#[cfg(feature = "native")]
fn route_dolby_vision_rpu(method: &str, args_json: &str) -> Outcome {
    match method {
        // args_json IS the request object for both of these
        "dolbyVisionRpuInfo" => opt_json(dolby_vision_rpu::dolby_vision_rpu_info_json(args_json)),
        "dolbyVisionConvertRpu" => {
            opt_json(dolby_vision_rpu::dolby_vision_convert_rpu_json(args_json))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_player_flow(method: &str, args_json: &str) -> Outcome {
    match method {
        "playerFlowDispatch" => {
            let args = object(args_json)?;
            opt_json(player_flow::player_flow_dispatch_json(
                field_str(&args, "state")?,
                field_str(&args, "action")?,
            ))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn route_player_scrobble(method: &str, args_json: &str) -> Outcome {
    match method {
        "scrobbleMediaContext" => opt_json(player_scrobble::scrobble_media_context_json(args_json)),
        "playerProgressPercent" => {
            let args = object(args_json)?;
            let position_ms = field(&args, "positionMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "positionMs must be a number"))?;
            let duration_ms = field(&args, "durationMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "durationMs must be a number"))?;
            Ok(json!(player_scrobble::progress_percent(
                position_ms,
                duration_ms,
            )))
        }
        "playerShouldSendScrobbleStart" => {
            let args = object(args_json)?;
            let token = args.get("token").and_then(Value::as_str);
            let is_playing = field(&args, "isPlaying")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "isPlaying must be bool"))?;
            let has_scrobbled_start = field(&args, "hasScrobbledStart")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "hasScrobbledStart must be bool"))?;
            let progress = field(&args, "progress")?
                .as_f64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "progress must be a number"))?
                as f32;
            Ok(json!(player_scrobble::should_send_start(
                token,
                is_playing,
                has_scrobbled_start,
                progress,
            )))
        }
        "playerShouldMarkScrobbleStopped" => {
            let args = object(args_json)?;
            let has_scrobbled_stop = field(&args, "hasScrobbledStop")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "hasScrobbledStop must be bool"))?;
            let progress = field(&args, "progress")?
                .as_f64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "progress must be a number"))?
                as f32;
            Ok(json!(player_scrobble::should_mark_stopped(
                has_scrobbled_stop,
                progress,
            )))
        }
        "playerShouldQueueScrobblePause" => {
            let args = object(args_json)?;
            let token = args.get("token").and_then(Value::as_str);
            let was_play_when_ready = field(&args, "wasPlayWhenReady")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "wasPlayWhenReady must be bool"))?;
            let has_scrobbled_start = field(&args, "hasScrobbledStart")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "hasScrobbledStart must be bool"))?;
            let has_scrobbled_stop = field(&args, "hasScrobbledStop")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "hasScrobbledStop must be bool"))?;
            Ok(json!(player_scrobble::should_queue_pause(
                token,
                was_play_when_ready,
                has_scrobbled_start,
                has_scrobbled_stop,
            )))
        }
        "playerShouldEnqueueDurableScrobble" => {
            let args = object(args_json)?;
            let token = args.get("token").and_then(Value::as_str);
            let progress = field(&args, "progress")?
                .as_f64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "progress must be a number"))?
                as f32;
            Ok(json!(player_scrobble::should_enqueue_durable(
                field_str(&args, "action")?,
                token,
                progress,
            )))
        }
        "playerShouldSavePeriodicProgress" => {
            let args = object(args_json)?;
            let is_playing = field(&args, "isPlaying")?
                .as_bool()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "isPlaying must be bool"))?;
            let now_ms = field(&args, "nowMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "nowMs must be a number"))?;
            let last_saved_at_ms = field(&args, "lastSavedAtMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "lastSavedAtMs must be a number"))?;
            Ok(json!(player_scrobble::should_save_periodic_progress(
                is_playing,
                now_ms,
                last_saved_at_ms,
            )))
        }
        "playerShouldSaveOnDispose" => {
            let args = object(args_json)?;
            let position_ms = field(&args, "positionMs")?
                .as_i64()
                .ok_or_else(|| fail(ErrorKind::InvalidArgs, "positionMs must be a number"))?;
            Ok(json!(player_scrobble::should_save_on_dispose(position_ms)))
        }

        _ => Err(fail(
            ErrorKind::UnknownMethod,
            format!("no such method `{method}`"),
        )),
    }
}

fn opt_str(value: Option<String>) -> Outcome {
    Ok(value.map(Value::String).unwrap_or(Value::Null))
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
            r#"{"meta":{"genres":["Anime"]},"episode":null,"stream":null,"addons":[]}"#,
        ));
        assert_eq!(detect["ok"], json!(true));
        assert_eq!(detect["value"]["confidence"], json!(65));

        let sync = parse(&core_invoke(
            "anilistEntriesToSync",
            r#"{"entries":[],"nowMs":0}"#,
        ));
        assert_eq!(sync["ok"], json!(true));
        assert_eq!(sync["value"]["watchlist"], json!([]));

        let merged = parse(&core_invoke(
            "mergeLibraryItemsById",
            r#"{"local":[],"incoming":[{"id":"a"}]}"#,
        ));
        assert_eq!(merged["value"][0]["id"], json!("a"));

        let plan = parse(&core_invoke(
            "tmdbPeopleRequestPlan",
            r#"{"meta":{"id":"tt123","type":"movie"},"apiKey":"k","language":"en"}"#,
        ));
        assert_eq!(
            plan["value"]["findUrl"],
            json!("https://api.themoviedb.org/3/find/tt123?api_key=k&language=en-US&external_source=imdb_id")
        );

        let images = parse(&core_invoke(
            "tmdbPeopleImagesFromCredits",
            r#"{"credits":{"cast":[{"name":"Jane Doe","profile_path":"/x.jpg"}]},"links":[{"name":"jane  doe"}]}"#,
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

    #[test]
    fn calendar_plan_methods_route_and_compute() {
        let candidates = parse(&core_invoke(
            "calendarSeasonCandidates",
            r#"{"seasonsCount":10,"lastVideoId":"tt1:2:3"}"#,
        ));
        assert_eq!(candidates["ok"], json!(true));
        assert_eq!(candidates["value"], json!([2, 3, 10]));

        let rows = parse(&core_invoke(
            "calendarWidgetRows",
            r#"{"items":[{"dateIso":"2026-07-18","title":"Show","seasonNumber":1,"episodeNumber":2}],"maxRows":4}"#,
        ));
        assert_eq!(rows["value"][0]["episodeText"], json!("S1E2"));

        let content = parse(&core_invoke(
            "calendarContentPlan",
            r#"{"items":[{"dateIso":"2026-07-18","metaId":"tt1","title":"Show"}],"monthPrefix":"2026-07"}"#,
        ));
        assert_eq!(content["value"][0]["metaId"], json!("tt1"));

        let notifications = parse(&core_invoke(
            "calendarNotificationContent",
            r#"{"items":[{"dateIso":"2026-07-18","metaId":"tt1","metaType":"series","title":"Show","seasonNumber":1,"episodeNumber":1}],"todayIso":"2026-07-18","alreadyNotifiedKeys":[]}"#,
        ));
        assert_eq!(
            notifications["value"]["items"][0]["titleKey"],
            json!("notification.new_season_released")
        );
        assert_eq!(notifications["value"]["keys"].as_array().unwrap().len(), 1);

        let released = parse(&core_invoke(
            "calendarReleaseDetection",
            r#"{"items":[{"dateIso":"2026-07-18"},{"dateIso":"2026-07-19"}],"todayIso":"2026-07-18"}"#,
        ));
        assert_eq!(released["value"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn newly_routed_modules_compute() {
        let input_type = parse(&core_invoke(
            "addonStoreInputType",
            r#"{"input":"https://example.com/addon/manifest.json"}"#,
        ));
        assert_eq!(input_type["value"], json!("stremio_manifest"));

        let secure = parse(&core_invoke(
            "isSecureRemoteUrl",
            r#"{"url":"http://example.com"}"#,
        ));
        assert_eq!(secure["value"], json!(false));

        let same = parse(&core_invoke(
            "samePluginRepositoryUrl",
            r#"{"left":"https://Example.com/repo/","right":"http://example.com/repo"}"#,
        ));
        assert_eq!(same["value"], json!(true));

        let buffer = parse(&core_invoke("safePlayerBufferCacheMb", r#"{"value":50}"#));
        assert_eq!(buffer["value"], json!(100));

        let dv_mode = parse(&core_invoke(
            "safeDolbyVisionFallbackMode",
            r#"{"mode":"dv8"}"#,
        ));
        assert_eq!(dv_mode["value"], json!("dv8"));

        let source_mode = parse(&core_invoke(
            "safeStreamSourceSelectionMode",
            r#"{"mode":"regex"}"#,
        ));
        assert_eq!(source_mode["value"], json!("regex"));

        let policy = parse(&core_invoke("directPlaybackPolicy", "{}"));
        assert_eq!(policy["value"]["metaDetailTimeoutMs"], json!(3500));

        let prefix = parse(&core_invoke(
            "streamDiscoveryCachePrefix",
            r#"{"contentType":"movie","id":"tt1","language":"en"}"#,
        ));
        assert_eq!(prefix["value"], json!("movie|tt1|en"));
    }

    #[test]
    fn gap_filled_routes_compute() {
        let bearer = parse(&core_invoke("traktBearer", r#"{"token":"abc"}"#));
        assert_eq!(bearer["value"], json!("Bearer abc"));

        let has_client = parse(&core_invoke("traktHasClient", r#"{"apiKey":""}"#));
        assert_eq!(has_client["value"], json!(false));

        let expires_at = parse(&core_invoke(
            "traktTokenExpiresAt",
            r#"{"createdAtSeconds":1000,"expiresInSeconds":7200}"#,
        ));
        assert_eq!(expires_at["value"], json!(1000 + 6900));

        let show_id = parse(&core_invoke(
            "traktShowIdFromEpisodeId",
            r#"{"videoId":"tt1:2:3"}"#,
        ));
        assert_eq!(show_id["value"], json!("tt1"));

        let episode_matches = parse(&core_invoke(
            "episodeTextMatches",
            r#"{"text":"Show S01E02","season":1,"episode":2}"#,
        ));
        assert_eq!(episode_matches["value"], json!(true));

        let stream_matches = parse(&core_invoke(
            "streamMatchesEpisode",
            r#"{"videoId":"tt1:1:2","title":"","name":"","description":"","filename":"Show.S01E02.mkv","effectiveFilename":""}"#,
        ));
        assert_eq!(stream_matches["value"], json!(true));

        let content_type = parse(&core_invoke("normalizeContentType", r#"{"value":"tv"}"#));
        assert_eq!(content_type["value"], json!("series"));

        let feed_part = parse(&core_invoke("stableFeedPart", r#"{"value":"Foo Bar!"}"#));
        assert_eq!(feed_part["value"], json!("foo_bar"));

        let base = parse(&core_invoke(
            "baseUrl",
            r#"{"url":"https://example.com/addon/manifest.json"}"#,
        ));
        assert_eq!(base["value"], json!("https://example.com/addon/"));

        let progress = parse(&core_invoke(
            "playerProgressPercent",
            r#"{"positionMs":50,"durationMs":100}"#,
        ));
        assert_eq!(progress["value"], json!(50.0));

        let should_save = parse(&core_invoke(
            "playerShouldSaveOnDispose",
            r#"{"positionMs":6000}"#,
        ));
        assert_eq!(should_save["value"], json!(true));

        let category_json =
            r#"{\"id\":\"a\",\"name\":\"A\",\"type\":\"movie\",\"items\":[{\"id\":\"tt1\"}]}"#;
        let overlap = parse(&core_invoke(
            "homeOverlapRatio",
            &format!(r#"{{"firstJson":"{category_json}","secondJson":"{category_json}"}}"#),
        ));
        assert_eq!(overlap["value"], json!(1.0));

        let select = parse(&core_invoke(
            "selectStreamIndex",
            r#"{"streamsJson":"[]","currentVideoId":"tt1","initialStreamIndex":0,"sourceSelectionMode":"manual"}"#,
        ));
        assert_eq!(select["value"], json!(-1));

        let ids = parse(&core_invoke(
            "streamRequestIds",
            r#"{"contentType":"movie","id":"tt1"}"#,
        ));
        assert_eq!(ids["value"], json!(["tt1"]));
    }

    #[test]
    fn last_gap_filled_routes_compute() {
        let locator = parse(&core_invoke(
            "parseEpisodeLocator",
            r#"{"input":"tt1:2:3"}"#,
        ));
        assert_eq!(locator["value"]["baseId"], json!("tt1"));
        assert_eq!(locator["value"]["season"], json!(2));
        assert_eq!(locator["value"]["episode"], json!(3));

        let no_locator = parse(&core_invoke("parseEpisodeLocator", r#"{"input":"nope"}"#));
        assert_eq!(no_locator["value"], Value::Null);

        let audio = parse(&core_invoke(
            "resolvePreferredAudioLanguage",
            r#"{"lastAudioLanguage":null,"preferredAudioLanguage":"en","originalLanguage":"ja"}"#,
        ));
        assert_eq!(audio["value"], json!("ja"));

        let subtitle_match = parse(&core_invoke(
            "subtitleLanguageMatches",
            r#"{"label":"english","language":null,"preferredLanguage":"en"}"#,
        ));
        assert_eq!(subtitle_match["value"], json!(true));

        let toggled = parse(&core_invoke(
            "toggleMetadataFeed",
            r#"{"selectedKeys":"[]","availableKeys":"[\"a\"]","key":"a"}"#,
        ));
        assert_eq!(toggled["value"], json!(["a"]));

        let manifest_request = json!({
            "body": json!({"resources": ["catalog"], "types": ["movie"]}).to_string(),
            "transportUrl": "https://example.com/manifest.json",
            "unknownName": "Unknown Addon"
        });
        let manifest = parse(&core_invoke("parseManifest", &manifest_request.to_string()));
        assert_eq!(manifest["ok"], json!(true));
        assert_eq!(
            manifest["value"]["manifest"]["name"],
            json!("Unknown Addon")
        );
    }

    // tests/wire/core_invoke_methods.txt is a checked-in list of every method
    // name core_invoke routes. It exists so renaming or removing one shows up
    // as a failure in this repo (a diff in this fixture is the review
    // artifact for an intentional rename) instead of as a runtime
    // "no such method" discovered on a platform we can't see from here. This
    // doesn't verify each method's business logic — just that the name is
    // still recognized rather than falling through every router to
    // UnknownMethod.
    #[test]
    fn every_known_core_invoke_method_still_routes() {
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/wire/core_invoke_methods.txt");
        let methods = std::fs::read_to_string(&fixture_path)
            .unwrap_or_else(|_| panic!("missing fixture {fixture_path:?}"));
        let methods: Vec<&str> = methods
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        assert!(!methods.is_empty(), "fixture list must not be empty");

        for method in methods {
            let result = parse(&core_invoke(method, "{}"));
            let kind = result["error"]["kind"].as_str().unwrap_or("");
            assert_ne!(
                kind, "unknown_method",
                "{method} no longer routes anywhere — renamed or removed?"
            );
        }
    }
}
