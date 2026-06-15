use crate::addon_protocol;
use crate::addon_resource;
use crate::addon_store;
use crate::app_state;
use crate::calendar_plan;
use crate::content_identity;
use crate::core_contract;
use crate::data_policy;
use crate::discovery_plan;
use crate::external_sync;
use crate::headless_adapter_plan;
use crate::headless_engine;
use crate::home_ranking;
use crate::intro_segments;
use crate::library_state;
use crate::offline_download;
use crate::platform_plan;
use crate::player_flow;
use crate::player_policy;
use crate::player_scrobble;
use crate::profile_prefs;
use crate::profile_contract;
use crate::repository_flow;
use crate::stream_policy;
use crate::search_plan;
use crate::tmdb_plan;
use crate::watchlist_plan;
use serde_json::json;

pub struct FluxaCore;

impl FluxaCore {
    pub fn normalize_manifest_url(raw_url: &str) -> String {
        addon_protocol::normalize_manifest_url(raw_url)
    }

    pub fn identity(raw_url: &str) -> String {
        addon_protocol::identity(raw_url)
    }

    pub fn manifest_candidates(raw_url: &str) -> Vec<String> {
        addon_protocol::manifest_candidates(raw_url)
    }

    pub fn manifest_fetch_plan_json(raw_url: &str) -> Option<String> {
        addon_protocol::manifest_fetch_plan_json(raw_url)
    }

    pub fn base_url(raw_url: &str) -> String {
        addon_protocol::base_url(raw_url)
    }

    pub fn prefer_https_asset_url(raw_url: &str) -> Option<String> {
        addon_protocol::prefer_https_asset_url(raw_url)
    }

    pub fn build_resource_url(
        transport_url: &str,
        resource: &str,
        content_type: &str,
        id: &str,
        extra_json: Option<&str>,
    ) -> String {
        addon_protocol::build_resource_url(transport_url, resource, content_type, id, extra_json)
    }

    pub fn parse_manifest_json(
        body: &str,
        transport_url: &str,
        unknown_name: &str,
    ) -> Option<String> {
        addon_protocol::parse_manifest(body, transport_url, unknown_name)
    }

    pub fn resolve_manifest_assets_json(descriptor_json: &str) -> Option<String> {
        addon_protocol::resolve_manifest_assets_json(descriptor_json)
    }

    pub fn merge_live_manifest_json(
        descriptor_json: &str,
        live_json: Option<&str>,
        unknown_name: &str,
    ) -> Option<String> {
        addon_protocol::merge_live_manifest_json(descriptor_json, live_json, unknown_name)
    }

    pub fn supports_resource(
        manifest_json: &str,
        resource_name: &str,
        content_type: Option<&str>,
        id: Option<&str>,
    ) -> bool {
        addon_protocol::supports_resource(manifest_json, resource_name, content_type, id)
    }

    pub fn catalog_supports_extra(catalog_json: &str, extra_name: &str) -> bool {
        addon_protocol::catalog_supports_extra(catalog_json, extra_name)
    }

    pub fn catalog_requires_extra(catalog_json: &str, extra_name: &str) -> bool {
        addon_protocol::catalog_requires_extra(catalog_json, extra_name)
    }

    pub fn catalog_has_required_extra_except(catalog_json: &str, allowed_names_json: &str) -> bool {
        addon_protocol::catalog_has_required_extra_except(catalog_json, allowed_names_json)
    }

    pub fn parse_addon_resource_result_json(
        resource: &str,
        url: &str,
        status_code: i32,
        body: Option<&str>,
    ) -> String {
        addon_resource::parse_addon_resource_result_json(resource, url, status_code, body)
    }

    pub fn normalize_addon_subtitles_json(subtitles_json: &str, resource_url: &str) -> String {
        addon_resource::normalize_addon_subtitles_json(subtitles_json, resource_url)
    }

    pub fn cache_entry_policy_json(request_json: &str) -> Option<String> {
        data_policy::cache_entry_policy_json(request_json)
    }

    pub fn cache_trim_policy_json(request_json: &str) -> Option<String> {
        data_policy::cache_trim_policy_json(request_json)
    }

    pub fn data_failure_policy_json(request_json: &str) -> Option<String> {
        data_policy::data_failure_policy_json(request_json)
    }

    pub fn headless_provider_availability_plan_json(request_json: &str) -> Option<String> {
        headless_adapter_plan::provider_availability_plan_json(request_json)
    }

    pub fn headless_detail_stream_result_plan_json(request_json: &str) -> Option<String> {
        headless_adapter_plan::detail_stream_result_plan_json(request_json)
    }

    pub fn headless_prefetch_detail_streams_plan_json(request_json: &str) -> Option<String> {
        headless_adapter_plan::prefetch_detail_streams_plan_json(request_json)
    }

    pub fn headless_direct_playback_policy_json() -> String {
        headless_adapter_plan::direct_playback_policy_json()
    }

    pub fn create_app_core_state(initial_json: &str) -> u64 {
        app_state::create_app_core_state(initial_json)
    }

    pub fn destroy_app_core_state(handle: u64) -> bool {
        app_state::destroy_app_core_state(handle)
    }

    pub fn app_core_state_json(handle: u64) -> Option<String> {
        app_state::app_core_state_json(handle)
    }

    pub fn app_core_dispatch_json(handle: u64, action_json: &str) -> Option<String> {
        app_state::app_core_dispatch_json(handle, action_json)
    }

    pub fn create_headless_engine(initial_json: &str) -> u64 {
        headless_engine::create_headless_engine(initial_json)
    }

    pub fn destroy_headless_engine(handle: u64) -> bool {
        headless_engine::destroy_headless_engine(handle)
    }

    pub fn headless_engine_snapshot_json(handle: u64) -> Option<String> {
        headless_engine::headless_engine_snapshot_json(handle)
    }

    pub fn headless_engine_dispatch_json(handle: u64, action_json: &str) -> Option<String> {
        headless_engine::headless_engine_dispatch_json(handle, action_json)
    }

    pub fn headless_engine_complete_effect_json(handle: u64, result_json: &str) -> Option<String> {
        headless_engine::headless_engine_complete_effect_json(handle, result_json)
    }

    pub fn core_capabilities_json(portable: bool) -> String {
        core_contract::core_capabilities_json(portable)
    }

    pub fn addon_store_input_type(raw: &str) -> &'static str {
        addon_store::addon_store_input_type(raw)
    }

    pub fn normalize_cloudstream_repo_url(raw: &str) -> String {
        addon_store::normalize_cloudstream_repo_url(raw)
    }

    pub fn normalize_plugin_repository_url(raw: &str) -> String {
        addon_store::normalize_plugin_repository_url(raw)
    }

    pub fn is_secure_remote_url(raw: &str) -> bool {
        addon_store::is_secure_remote_url(raw)
    }

    pub fn same_plugin_repository_url(left: &str, right: &str) -> bool {
        addon_store::same_plugin_repository_url(left, right)
    }

    pub fn profile_local_addons_key_json(profile_json: &str) -> Option<String> {
        addon_store::profile_local_addons_key_json(profile_json)
    }

    pub fn sanitize_profile_json(
        profile_json: &str,
        mirrored_addons_json: &str,
        merge_mirrored_addons: bool,
    ) -> Option<String> {
        addon_store::sanitize_profile_json(
            profile_json,
            mirrored_addons_json,
            merge_mirrored_addons,
        )
    }

    pub fn addon_store_search_policy_json(request_json: &str) -> Option<String> {
        addon_store::addon_store_search_policy_json(request_json)
    }

    pub fn extract_addon_manifest_url(detail_text: &str) -> Option<String> {
        addon_store::extract_addon_manifest_url(detail_text)
    }

    pub fn search_result_grouping_json(request_json: &str) -> Option<String> {
        search_plan::search_result_grouping_json(request_json)
    }

    pub fn build_metadata_feed_options_json(addons_json: &str) -> Option<String> {
        search_plan::build_metadata_feed_options_json(addons_json)
    }

    pub fn discover_catalog_options_json(addons_json: &str, selected_type: &str) -> Option<String> {
        search_plan::discover_catalog_options_json(addons_json, selected_type)
    }

    pub fn discover_sort_plan_json(request_json: &str) -> Option<String> {
        search_plan::discover_sort_plan_json(request_json)
    }

    pub fn library_sort_plan_json(request_json: &str) -> Option<String> {
        search_plan::library_sort_plan_json(request_json)
    }

    pub fn detail_series_lookup_id(raw_id: &str) -> String {
        search_plan::detail_series_lookup_id(raw_id)
    }

    pub fn detail_season_load_plan_json(request_json: &str) -> Option<String> {
        search_plan::detail_season_load_plan_json(request_json)
    }

    pub fn player_backend_selection_json(request_json: &str) -> Option<String> {
        player_policy::player_backend_selection_json(request_json)
    }

    pub fn torrent_fallback_file_policy_json(request_json: &str) -> Option<String> {
        player_policy::torrent_fallback_file_policy_json(request_json)
    }

    pub fn player_buffer_targets_json(request_json: &str) -> Option<String> {
        player_policy::player_buffer_targets_json(request_json)
    }

    pub fn player_retry_policy_json(request_json: &str) -> Option<String> {
        player_policy::player_retry_policy_json(request_json)
    }

    pub fn player_source_sidebar_plan_json(request_json: &str) -> Option<String> {
        player_policy::player_source_sidebar_plan_json(request_json)
    }

    pub fn calendar_content_plan_json(request_json: &str) -> Option<String> {
        calendar_plan::calendar_content_plan_json(request_json)
    }

    pub fn calendar_season_candidates_json(request_json: &str) -> Option<String> {
        calendar_plan::calendar_season_candidates_json(request_json)
    }

    pub fn calendar_widget_rows_json(request_json: &str) -> Option<String> {
        calendar_plan::calendar_widget_rows_json(request_json)
    }

    pub fn calendar_notification_content_json(request_json: &str) -> Option<String> {
        calendar_plan::calendar_notification_content_json(request_json)
    }

    pub fn calendar_release_detection_json(request_json: &str) -> Option<String> {
        calendar_plan::calendar_release_detection_json(request_json)
    }

    pub fn repository_meta_detail_plan_json(request_json: &str) -> Option<String> {
        repository_flow::repository_meta_detail_plan_json(request_json)
    }

    pub fn repository_season_videos_json(meta_detail_json: &str, season_number: i32) -> String {
        repository_flow::repository_season_videos_json(meta_detail_json, season_number)
    }

    pub fn manifest_fetch_decision_json(request_json: &str) -> Option<String> {
        repository_flow::manifest_fetch_decision_json(request_json)
    }

    pub fn addon_resource_request_plan_json(request_json: &str) -> Option<String> {
        repository_flow::addon_resource_request_plan_json(request_json)
    }

    pub fn resource_fetch_plan_json(request_json: &str) -> Option<String> {
        platform_plan::resource_fetch_plan_json(request_json)
    }

    pub fn resource_parse_plan_json(request_json: &str) -> Option<String> {
        platform_plan::resource_parse_plan_json(request_json)
    }

    pub fn playback_prepare_plan_json(request_json: &str) -> Option<String> {
        platform_plan::playback_prepare_plan_json(request_json)
    }

    pub fn library_local_state_plan_json(request_json: &str) -> Option<String> {
        platform_plan::library_local_state_plan_json(request_json)
    }

    pub fn preferences_schema_json() -> String {
        platform_plan::preferences_schema_json()
    }

    pub fn apply_preference_update_json(request_json: &str) -> Option<String> {
        platform_plan::apply_preference_update_json(request_json)
    }

    pub fn addon_collection_mutation_plan_json(request_json: &str) -> Option<String> {
        platform_plan::addon_collection_mutation_plan_json(request_json)
    }

    pub fn detail_episode_plan_json(request_json: &str) -> Option<String> {
        platform_plan::detail_episode_plan_json(request_json)
    }

    pub fn addon_streams_with_provider_json(streams_json: &str, addon_name: &str) -> String {
        repository_flow::addon_streams_with_provider_json(streams_json, addon_name)
    }

    pub fn active_profile_plan_json(request_json: &str) -> Option<String> {
        profile_contract::active_profile_plan_json(request_json)
    }

    pub fn token_merge_plan_json(request_json: &str) -> Option<String> {
        profile_contract::token_merge_plan_json(request_json)
    }

    pub fn profile_default_seed_json(request_json: &str) -> Option<String> {
        profile_contract::profile_default_seed_json(request_json)
    }

    pub fn profile_settings_migration_plan_json(request_json: &str) -> Option<String> {
        profile_contract::profile_settings_migration_plan_json(request_json)
    }

    pub fn profile_avatar_default_json(request_json: &str) -> Option<String> {
        profile_contract::profile_avatar_default_json(request_json)
    }

    pub fn watchlist_toggle_plan_json(request_json: &str) -> Option<String> {
        watchlist_plan::watchlist_toggle_plan_json(request_json)
    }

    pub fn library_external_merge_plan_json(request_json: &str) -> Option<String> {
        watchlist_plan::library_external_merge_plan_json(request_json)
    }

    pub fn library_collection_import_validation_json(request_json: &str) -> Option<String> {
        watchlist_plan::library_collection_import_validation_json(request_json)
    }

    pub fn library_offline_grouping_json(request_json: &str) -> Option<String> {
        watchlist_plan::library_offline_grouping_json(request_json)
    }

    pub fn playback_progress_merge_plan_json(request_json: &str) -> Option<String> {
        watchlist_plan::playback_progress_merge_plan_json(request_json)
    }

    pub fn parse_episode_locator_json(raw: &str) -> Option<String> {
        let (base_id, season, episode) = content_identity::parse_episode_locator(raw)?;
        serde_json::to_string(&json!({
            "baseId": base_id,
            "season": season,
            "episode": episode
        }))
        .ok()
    }

    pub fn stream_request_ids_json(
        content_type: &str,
        id: &str,
        detail_id: Option<&str>,
        current_series_lookup_id: Option<&str>,
        canonical_base_id: Option<&str>,
    ) -> Option<String> {
        serde_json::to_string(&content_identity::stream_request_ids(
            content_type,
            id,
            detail_id,
            current_series_lookup_id,
            canonical_base_id,
        ))
        .ok()
    }

    pub fn playback_intro_lookup_content_id(id: &str) -> String {
        content_identity::playback_intro_lookup_content_id(id)
    }

    pub fn playback_stream_request_ids_json(
        content_type: &str,
        id: &str,
        detail_id: Option<&str>,
    ) -> Option<String> {
        content_identity::playback_stream_request_ids_json(content_type, id, detail_id)
    }

    pub fn direct_playback_plan_json(
        meta_json: &str,
        detail_json: Option<&str>,
        today_iso: &str,
    ) -> Option<String> {
        content_identity::direct_playback_plan_json(meta_json, detail_json, today_iso)
    }

    pub fn stream_discovery_episode_context_json(
        content_type: &str,
        request_id: &str,
        detail_json: Option<&str>,
        season_episodes_json: &str,
    ) -> Option<String> {
        content_identity::stream_discovery_episode_context_json(
            content_type,
            request_id,
            detail_json,
            season_episodes_json,
        )
    }

    pub fn stream_playback_info_json(stream_json: &str) -> Option<String> {
        stream_policy::stream_playback_info_json(stream_json)
    }

    pub fn stream_request_headers_json(headers_json: &str) -> Option<String> {
        stream_policy::stream_request_headers_json(headers_json)
    }

    pub fn stream_request_referer(url: &str) -> Option<String> {
        stream_policy::stream_request_referer(url)
    }

    pub fn episode_text_matches(text: &str, season: i32, episode: i32) -> bool {
        content_identity::text_matches_episode(text, season, episode)
    }

    pub fn stream_matches_episode(video_id: &str, fields: &[String]) -> bool {
        content_identity::stream_matches_episode(video_id, fields)
    }

    pub fn select_stream_index(
        streams_json: &str,
        current_video_id: &str,
        initial_stream_index: i32,
        saved_url: Option<&str>,
        saved_title: Option<&str>,
        source_selection_mode: &str,
        regex_pattern: Option<&str>,
        preferred_binge_group: Option<&str>,
    ) -> i32 {
        stream_policy::select_stream_index(
            streams_json,
            current_video_id,
            initial_stream_index,
            saved_url,
            saved_title,
            source_selection_mode,
            regex_pattern,
            preferred_binge_group,
        )
    }

    pub fn merge_continue_watching_duplicates_json(items_json: &str) -> Option<String> {
        content_identity::merge_continue_watching_duplicates_json(items_json)
    }

    pub fn filter_discover_results_json(
        items_json: &str,
        year: Option<&str>,
        rating: Option<f32>,
        region: Option<&str>,
    ) -> Option<String> {
        content_identity::filter_discover_results_json(items_json, year, rating, region)
    }

    pub fn resolve_preferred_audio_language(
        last_audio_language: Option<&str>,
        preferred_audio_language: Option<&str>,
        original_language: Option<&str>,
    ) -> String {
        stream_policy::resolve_preferred_audio_language(
            last_audio_language,
            preferred_audio_language,
            original_language,
        )
    }

    pub fn subtitle_language_matches(
        label: &str,
        language: Option<&str>,
        preferred_language: &str,
    ) -> bool {
        stream_policy::subtitle_language_matches(label, language, preferred_language)
    }

    pub fn find_preferred_subtitle_index(
        tracks_json: &str,
        last_subtitle_language: Option<&str>,
        preferred_subtitle_language: Option<&str>,
        secondary_subtitle_language: Option<&str>,
    ) -> i32 {
        stream_policy::find_preferred_subtitle_index(
            tracks_json,
            last_subtitle_language,
            preferred_subtitle_language,
            secondary_subtitle_language,
        )
    }

    pub fn player_track_state_json(request_json: &str) -> Option<String> {
        stream_policy::player_track_state_json(request_json)
    }

    pub fn torrent_runtime_info_json(request_json: &str) -> Option<String> {
        stream_policy::torrent_runtime_info_json(request_json)
    }

    pub fn torrent_status_info_json(status_json: &str) -> Option<String> {
        stream_policy::torrent_status_info_json(status_json)
    }

    pub fn stable_feed_part(value: &str) -> String {
        content_identity::stable_feed_part(value)
    }

    pub fn normalize_content_type(value: &str) -> Option<&'static str> {
        content_identity::normalize_content_type(value)
    }

    pub fn parse_extra_args_json(extra: &str) -> Option<String> {
        content_identity::parse_extra_args_json(extra)
    }

    pub fn provider_search_terms(provider: &str) -> Vec<String> {
        content_identity::provider_search_terms(provider)
    }

    pub fn effective_metadata_feed_selection_json(
        selected_keys_json: &str,
        available_keys_json: &str,
    ) -> Option<String> {
        content_identity::effective_metadata_feed_selection_json(
            selected_keys_json,
            available_keys_json,
        )
    }

    pub fn toggle_metadata_feed_json(
        selected_keys_json: &str,
        available_keys_json: &str,
        key: &str,
    ) -> Option<String> {
        content_identity::toggle_metadata_feed_json(selected_keys_json, available_keys_json, key)
    }

    pub fn toggle_metadata_feed_limited_json(
        selected_keys_json: &str,
        available_keys_json: &str,
        key: &str,
        max_enabled: i32,
    ) -> Option<String> {
        content_identity::toggle_metadata_feed_limited_json(
            selected_keys_json,
            available_keys_json,
            key,
            max_enabled,
        )
    }

    pub fn set_metadata_feed_group_enabled_json(
        selected_keys_json: &str,
        available_keys_json: &str,
        group_keys_json: &str,
        enabled: bool,
    ) -> Option<String> {
        content_identity::set_metadata_feed_group_enabled_json(
            selected_keys_json,
            available_keys_json,
            group_keys_json,
            enabled,
        )
    }

    pub fn ordered_metadata_feed_keys(option_keys_json: &str, order_json: &str) -> Option<String> {
        content_identity::ordered_metadata_feed_keys(option_keys_json, order_json)
    }

    pub fn move_metadata_feed_order_json(
        option_keys_json: &str,
        current_order_json: &str,
        key: &str,
        delta: i32,
    ) -> Option<String> {
        content_identity::move_metadata_feed_order_json(
            option_keys_json,
            current_order_json,
            key,
            delta,
        )
    }

    pub fn content_trakt_key(meta_json: &str) -> Option<String> {
        content_identity::content_trakt_key(meta_json)
    }

    pub fn content_billboard_key(meta_json: &str) -> Option<String> {
        content_identity::content_billboard_key(meta_json)
    }

    pub fn content_merge_keys_json(meta_json: &str) -> Option<String> {
        content_identity::content_keys_json(meta_json, false)
    }

    pub fn content_watched_keys_json(meta_json: &str) -> Option<String> {
        content_identity::content_keys_json(meta_json, true)
    }

    pub fn episode_filename_candidate(stream_json: &str, video_id: &str) -> Option<String> {
        content_identity::episode_filename_candidate(stream_json, video_id)
    }

    pub fn stream_discovery_cache_key(request_json: &str) -> Option<String> {
        content_identity::stream_discovery_cache_key(request_json)
    }

    pub fn stream_discovery_plan_json(request_json: &str) -> Option<String> {
        discovery_plan::stream_discovery_plan_json(request_json)
    }

    pub fn stream_discovery_execution_policy_json(request_json: &str) -> Option<String> {
        discovery_plan::stream_discovery_execution_policy_json(request_json)
    }

    pub fn stream_discovery_cache_prefix(content_type: &str, id: &str, language: &str) -> String {
        discovery_plan::stream_discovery_cache_prefix(content_type, id, language)
    }

    pub fn discover_catalog_cache_key(request_json: &str) -> Option<String> {
        content_identity::discover_catalog_cache_key(request_json)
    }

    pub fn curate_home_items_json(category_json: &str) -> Option<String> {
        home_ranking::curate_home_items_json(category_json)
    }

    pub fn home_overlap_ratio_json(first_json: &str, second_json: &str) -> Option<f32> {
        home_ranking::home_overlap_ratio_json(first_json, second_json)
    }

    pub fn home_personalization_score_json(
        category_json: &str,
        preferred_genres_json: &str,
        preferred_types_json: &str,
        priority_labels_json: &str,
    ) -> Option<i32> {
        home_ranking::home_personalization_score_json(
            category_json,
            preferred_genres_json,
            preferred_types_json,
            priority_labels_json,
        )
    }

    pub fn prioritize_home_rows_json(
        categories_json: &str,
        preferred_order_labels_json: &str,
        preferred_genres_json: &str,
        preferred_types_json: &str,
        priority_labels_json: &str,
    ) -> Option<String> {
        home_ranking::home_prioritize_rows_json(
            categories_json,
            preferred_order_labels_json,
            preferred_genres_json,
            preferred_types_json,
            priority_labels_json,
        )
    }

    pub fn optimize_home_rows_json(request_json: &str) -> Option<String> {
        home_ranking::optimize_home_rows_json(request_json)
    }

    pub fn billboard_score_candidate_json(
        meta_json: &str,
        days_since_release: Option<i64>,
    ) -> Option<i32> {
        home_ranking::billboard_score_candidate_json(meta_json, days_since_release)
    }

    pub fn billboard_has_backdrop_candidate_json(meta_json: &str) -> bool {
        home_ranking::has_billboard_backdrop_candidate_json(meta_json)
    }

    pub fn billboard_visual_score_json(meta_json: &str) -> Option<i32> {
        home_ranking::billboard_visual_score_json(meta_json)
    }

    pub fn billboard_editorial_match_score_json(meta_json: &str, spec_json: &str) -> Option<i32> {
        home_ranking::billboard_editorial_match_score_json(meta_json, spec_json)
    }

    pub fn build_billboard_pool_json(
        enriched_json: &str,
        candidates_json: &str,
    ) -> Option<String> {
        home_ranking::build_billboard_pool_json(enriched_json, candidates_json)
    }

    pub fn normalize_home_catalog_items_json(
        items_json: &str,
        catalog_id: &str,
        genre: Option<&str>,
        today_iso: &str,
    ) -> Option<String> {
        home_ranking::normalize_home_catalog_items_json(items_json, catalog_id, genre, today_iso)
    }

    pub fn player_progress_percent(position_ms: i64, duration_ms: i64) -> f32 {
        player_scrobble::progress_percent(position_ms, duration_ms)
    }

    pub fn player_should_send_scrobble_start(
        token: Option<&str>,
        is_playing: bool,
        has_scrobbled_start: bool,
        progress: f32,
    ) -> bool {
        player_scrobble::should_send_start(token, is_playing, has_scrobbled_start, progress)
    }

    pub fn player_should_mark_scrobble_stopped(has_scrobbled_stop: bool, progress: f32) -> bool {
        player_scrobble::should_mark_stopped(has_scrobbled_stop, progress)
    }

    pub fn player_should_queue_scrobble_pause(
        token: Option<&str>,
        was_play_when_ready: bool,
        has_scrobbled_start: bool,
        has_scrobbled_stop: bool,
    ) -> bool {
        player_scrobble::should_queue_pause(
            token,
            was_play_when_ready,
            has_scrobbled_start,
            has_scrobbled_stop,
        )
    }

    pub fn player_should_enqueue_durable_scrobble(
        action: &str,
        token: Option<&str>,
        progress: f32,
    ) -> bool {
        player_scrobble::should_enqueue_durable(action, token, progress)
    }

    pub fn player_should_save_periodic_progress(
        is_playing: bool,
        now_ms: i64,
        last_saved_at_ms: i64,
    ) -> bool {
        player_scrobble::should_save_periodic_progress(is_playing, now_ms, last_saved_at_ms)
    }

    pub fn player_should_save_on_dispose(position_ms: i64) -> bool {
        player_scrobble::should_save_on_dispose(position_ms)
    }

    pub fn safe_player_buffer_cache_mb(value: Option<i32>) -> i32 {
        profile_prefs::safe_player_buffer_cache_mb(value)
    }

    pub fn safe_dolby_vision_fallback_mode(
        mode: Option<&str>,
        legacy_dv7_fallback: Option<bool>,
        legacy_dv7_to_dv8_fallback: Option<bool>,
    ) -> &'static str {
        profile_prefs::safe_dolby_vision_fallback_mode(
            mode,
            legacy_dv7_fallback,
            legacy_dv7_to_dv8_fallback,
        )
    }

    pub fn safe_stream_source_selection_mode(mode: Option<&str>) -> &'static str {
        profile_prefs::safe_stream_source_selection_mode(mode)
    }

    pub fn profile_safe_prefs_json(profile_json: &str) -> Option<String> {
        profile_prefs::profile_safe_prefs_json(profile_json)
    }

    pub fn player_flow_dispatch_json(state_json: &str, action_json: &str) -> Option<String> {
        player_flow::player_flow_dispatch_json(state_json, action_json)
    }

    pub fn trakt_has_client(api_key: &str) -> bool {
        external_sync::trakt_has_client(api_key)
    }

    pub fn trakt_bearer(token: &str) -> String {
        external_sync::trakt_bearer(token)
    }

    pub fn trakt_scrobble_url(action: &str) -> String {
        external_sync::trakt_scrobble_url(action)
    }

    pub fn trakt_playback_url(content_type: Option<&str>) -> String {
        external_sync::trakt_playback_url(content_type)
    }

    pub fn trakt_token_expires_at(created_at_seconds: i64, expires_in_seconds: i64) -> i64 {
        external_sync::trakt_token_expires_at(created_at_seconds, expires_in_seconds)
    }

    pub fn trakt_oauth_error_code(body: &str) -> Option<String> {
        external_sync::trakt_oauth_error_code(body)
    }

    pub fn trakt_content_id_from_ids_json(ids_json: &str) -> Option<String> {
        external_sync::trakt_content_id_from_ids_json(ids_json)
    }

    pub fn trakt_ids_from_content_id_json(raw_id: &str) -> Option<String> {
        external_sync::trakt_ids_from_content_id_json(raw_id)
    }

    pub fn trakt_episode_locator_json(video_id: &str) -> Option<String> {
        external_sync::trakt_episode_locator_json(video_id)
    }

    pub fn trakt_show_id_from_episode_id(video_id: &str) -> String {
        external_sync::trakt_show_id_from_episode_id(video_id)
    }

    pub fn trakt_scrobble_media_id(
        parent_id: &str,
        video_id: Option<&str>,
        media_type: &str,
    ) -> String {
        external_sync::trakt_scrobble_media_id(parent_id, video_id, media_type)
    }

    pub fn trakt_history_request_json(meta_json: &str, episodes_json: &str) -> Option<String> {
        external_sync::trakt_history_request_json(meta_json, episodes_json)
    }

    pub fn playback_progress_item_json(
        meta_json: &str,
        time_offset: i64,
        duration: i64,
        now_utc: &str,
    ) -> Option<String> {
        library_state::playback_progress_item_json(meta_json, time_offset, duration, now_utc)
    }

    pub fn clear_playback_progress_item_json(meta_json: &str) -> Option<String> {
        library_state::clear_playback_progress_item_json(meta_json)
    }

    pub fn watched_state_items_json(
        meta_json: &str,
        episodes_json: &str,
        watched: bool,
        watched_at: Option<&str>,
    ) -> Option<String> {
        library_state::watched_state_items_json(meta_json, episodes_json, watched, watched_at)
    }

    pub fn library_continue_watching_items_json(items_json: &str) -> Option<String> {
        library_state::library_continue_watching_items_json(items_json)
    }

    pub fn filter_home_continue_watching_json(
        items_json: &str,
        trakt_watched_json: &str,
    ) -> Option<String> {
        library_state::filter_home_continue_watching_json(items_json, trakt_watched_json)
    }

    pub fn watched_video_ids_json(items_json: &str, imdb_id: &str) -> Option<String> {
        library_state::watched_video_ids_json(items_json, imdb_id)
    }

    pub fn offline_download_plan_json(request_json: &str) -> Option<String> {
        offline_download::offline_download_plan_json(request_json)
    }

    // content_identity extras

    pub fn parse_video_id_json(id: &str) -> String {
        content_identity::parse_video_id_json(id)
    }

    pub fn build_trakt_ids_json(video_id: &str) -> Option<String> {
        content_identity::build_trakt_ids_json(video_id)
    }

    // calendar extras

    pub fn calendar_items_from_meta_json(meta_json: &str, month_prefix: &str) -> Option<String> {
        calendar_plan::calendar_items_from_meta_json(meta_json, month_prefix)
    }

    pub fn calendar_item_matches_month_json(item_json: &str, month_prefix: &str) -> bool {
        calendar_plan::calendar_item_matches_month_json(item_json, month_prefix)
    }

    // external_sync: Trakt high-level

    pub fn trakt_playback_items_to_library_json(items_json: &str) -> Option<String> {
        external_sync::trakt_playback_items_to_library_json(items_json)
    }

    pub fn trakt_watchlist_to_items_json(movies_json: &str, shows_json: &str) -> Option<String> {
        external_sync::trakt_watchlist_to_items_json(movies_json, shows_json)
    }

    pub fn trakt_watched_to_ids_json(movies_json: &str, shows_json: &str) -> Option<String> {
        external_sync::trakt_watched_to_ids_json(movies_json, shows_json)
    }

    pub fn merge_external_watchlist_json(local_json: &str, external_json: &str) -> String {
        external_sync::merge_external_watchlist_json(local_json, external_json)
    }

    pub fn merge_external_watched_json(local_json: &str, external_json: &str) -> String {
        external_sync::merge_external_watched_json(local_json, external_json)
    }

    pub fn merge_continue_watching_lists_json(
        local_json: &str,
        external_json: &str,
        progress_json: &str,
    ) -> Option<String> {
        external_sync::merge_continue_watching_lists_json(local_json, external_json, progress_json)
    }

    // external_sync: Simkl

    pub fn simkl_watching_to_items_json(shows_json: &str, movies_json: &str) -> Option<String> {
        external_sync::simkl_watching_to_items_json(shows_json, movies_json)
    }

    pub fn simkl_watchlist_to_items_json(shows_json: &str, movies_json: &str) -> Option<String> {
        external_sync::simkl_watchlist_to_items_json(shows_json, movies_json)
    }

    pub fn simkl_watched_to_ids_json(shows_json: &str, movies_json: &str) -> Option<String> {
        external_sync::simkl_watched_to_ids_json(shows_json, movies_json)
    }

    pub fn trakt_scrobble_plan_json(
        video_id: &str,
        is_episode: bool,
        season: Option<i64>,
        ep_number: Option<i64>,
        time_pos_sec: f64,
        duration_sec: f64,
    ) -> Option<String> {
        let ids_json = content_identity::build_trakt_ids_json(video_id)?;
        player_scrobble::trakt_scrobble_plan_json(&ids_json, is_episode, season, ep_number, time_pos_sec, duration_sec)
    }

    pub fn simkl_scrobble_body_json(
        ids_json: &str,
        is_episode: bool,
        season: i64,
        ep_number: i64,
        time_pos_sec: f64,
        duration_sec: f64,
    ) -> Option<String> {
        player_scrobble::simkl_scrobble_body_json(ids_json, is_episode, season, ep_number, time_pos_sec, duration_sec)
    }

    // library_state extras

    pub fn normalize_library_document_json(json: &str) -> String {
        library_state::normalize_library_document_json(json)
    }

    pub fn is_up_next_continue_watching_item_json(item_json: &str) -> bool {
        library_state::is_up_next_continue_watching_item_json(item_json)
    }

    pub fn build_continue_watching_from_progress_json(progress_json: &str) -> Option<String> {
        library_state::build_continue_watching_from_progress_json(progress_json)
    }

    pub fn remember_last_watched_episodes_json(lib_json: &str, watched_ids_json: &str) -> String {
        library_state::remember_last_watched_episodes_json(lib_json, watched_ids_json)
    }

    pub fn compute_continue_watching_badges_json(
        candidates_json: &str,
        videos_by_series_json: &str,
        last_watched_json: &str,
        now_ms: i64,
    ) -> Option<String> {
        library_state::compute_continue_watching_badges_json(
            candidates_json,
            videos_by_series_json,
            last_watched_json,
            now_ms,
        )
    }

    // tmdb_plan

    pub fn tmdb_content_type(content_type: &str) -> &str {
        tmdb_plan::tmdb_content_type(content_type)
    }

    pub fn tmdb_language(language: &str) -> String {
        tmdb_plan::tmdb_language(language)
    }

    pub fn tmdb_image_url(path: Option<&str>, size: &str) -> Option<String> {
        tmdb_plan::tmdb_image_url(path, size)
    }

    pub fn tmdb_meta_to_meta_json(item_json: &str, requested_type: &str, language: &str) -> Option<String> {
        tmdb_plan::tmdb_meta_to_meta_json(item_json, requested_type, language)
    }

    pub fn tmdb_video_to_trailer_json(video_json: &str) -> Option<String> {
        tmdb_plan::tmdb_video_to_trailer_json(video_json)
    }

    pub fn tmdb_bulk_metas_to_metas_json(
        items_json: &str,
        requested_type: &str,
        language: &str,
    ) -> Option<String> {
        tmdb_plan::tmdb_bulk_metas_to_metas_json(items_json, requested_type, language)
    }

    pub fn tmdb_bulk_videos_to_trailers_json(items_json: &str) -> Option<String> {
        tmdb_plan::tmdb_bulk_videos_to_trailers_json(items_json)
    }

    pub fn tmdb_resolve_id_hint(content_id: &str) -> (String, bool) {
        tmdb_plan::tmdb_resolve_id_hint(content_id)
    }

    // intro_segments

    pub fn parse_intro_db_segments_json(data_json: &str) -> Option<String> {
        intro_segments::parse_intro_db_segments_json(data_json)
    }

    pub fn parse_aniskip_results_json(results_json: &str) -> Option<String> {
        intro_segments::parse_aniskip_results_json(results_json)
    }

    pub fn unique_intro_segments_json(segments_a_json: &str, segments_b_json: &str) -> Option<String> {
        intro_segments::unique_intro_segments_json(segments_a_json, segments_b_json)
    }

    pub fn merge_intro_segments_json(sources_json: &str) -> Option<String> {
        intro_segments::merge_intro_segments_json(sources_json)
    }

    // library_state: episode navigation

    pub fn resolve_next_episode_json(
        videos_json: &str,
        current_season: i64,
        current_episode: i64,
        now_ms: i64,
        released_only: bool,
    ) -> Option<String> {
        library_state::resolve_next_episode_json(
            videos_json,
            current_season,
            current_episode,
            now_ms,
            released_only,
        )
    }

    pub fn format_episode_line_json(
        last_episode_name: Option<&str>,
        last_episode_season: Option<i64>,
        last_episode_number: Option<i64>,
        last_video_id: Option<&str>,
    ) -> String {
        library_state::format_episode_line_json(
            last_episode_name,
            last_episode_season,
            last_episode_number,
            last_video_id,
        )
    }

    pub fn select_continue_watching_artwork_json(
        item_json: &str,
        artwork_preference: &str,
        is_horizontal: bool,
    ) -> Option<String> {
        library_state::select_continue_watching_artwork_json(item_json, artwork_preference, is_horizontal)
    }

    // external_sync: continue watching

    pub fn replace_external_continue_watching_json(
        existing_json: &str,
        provider: Option<&str>,
        items_json: &str,
    ) -> String {
        external_sync::replace_external_continue_watching_json(existing_json, provider, items_json)
    }

    // platform_plan: resource protocol helpers

    pub fn resource_kind_to_resource_json(kind: &str, request_resource: Option<&str>, item_resource: Option<&str>) -> String {
        platform_plan::resource_kind_to_resource(kind, request_resource, item_resource)
    }

    pub fn wrap_addon_resource_response_json(resource: &str, payload_json: &str) -> String {
        platform_plan::wrap_addon_resource_response(resource, payload_json)
    }

    // player_policy: next episode prefetch

    pub fn can_prefetch_next_episode_json(prefs_json: &str, stream_json: &str) -> bool {
        player_policy::can_prefetch_next_episode_json(prefs_json, stream_json)
    }

    pub fn select_next_episode_stream_json(
        streams_json: &str,
        current_stream_json: &str,
        prefs_json: &str,
    ) -> Option<String> {
        player_policy::select_next_episode_stream_json(streams_json, current_stream_json, prefs_json)
    }

    // watchlist_plan: collections import/export

    pub fn import_collections_json(raw_json: &str) -> Option<String> {
        watchlist_plan::import_collections_json(raw_json)
    }

    pub fn export_collections_json(collections_json: &str) -> Option<String> {
        watchlist_plan::export_collections_json(collections_json)
    }

    // search_plan: catalog/transport resolution

    pub fn resolve_transport_url_json(source_json: &str, addons_json: &str) -> Option<String> {
        search_plan::resolve_transport_url_json(source_json, addons_json)
    }

    pub fn resolve_feed_option_genre_json(feed_option_json: &str, addons_json: &str) -> Option<String> {
        search_plan::resolve_feed_option_genre_json(feed_option_json, addons_json)
    }
}

#[cfg(test)]
mod tests {
    use super::FluxaCore;

    #[test]
    fn public_core_api_builds_stremio_resource_urls() {
        let url = FluxaCore::build_resource_url(
            "https://addon.example/root/manifest.json",
            "stream",
            "series",
            "tt123:1:2",
            Some(r#"{"search":"one two"}"#),
        );

        assert!(url.starts_with("https://addon.example/root/stream/series/tt123%3A1%3A2/"));
        assert!(url.ends_with(".json"));
        assert!(url.contains("search=one%20two"));
    }

    #[test]
    fn public_core_api_drives_app_state_without_jni() {
        let handle = FluxaCore::create_app_core_state(r#"{"player":{"currentStreamIndex":3}}"#);
        assert!(handle > 0);

        let snapshot = FluxaCore::app_core_dispatch_json(
            handle,
            r#"{"type":"playerResetForEpisode","videoId":"tt123:1:2"}"#,
        )
        .expect("state snapshot");

        assert!(snapshot.contains(r#""currentVideoId":"tt123:1:2""#));
        assert!(snapshot.contains(r#""currentStreamIndex":0"#));
        assert!(FluxaCore::destroy_app_core_state(handle));
    }

    #[test]
    fn public_core_api_owns_player_scrobble_decisions() {
        assert_eq!(FluxaCore::player_progress_percent(5_000, 10_000), 50.0);
        assert!(FluxaCore::player_should_send_scrobble_start(
            Some("token"),
            true,
            false,
            0.3,
        ));
        assert!(FluxaCore::player_should_mark_scrobble_stopped(false, 80.0));
        assert!(FluxaCore::player_should_queue_scrobble_pause(
            Some("token"),
            true,
            true,
            false,
        ));
        assert!(!FluxaCore::player_should_enqueue_durable_scrobble(
            "pause",
            Some("token"),
            0.5,
        ));
        assert!(FluxaCore::player_should_save_periodic_progress(
            true, 30_001, 0
        ));
        assert!(FluxaCore::player_should_save_on_dispose(5_001));
    }


    #[test]
    fn public_core_api_normalizes_profile_playback_preferences() {
        assert_eq!(FluxaCore::safe_player_buffer_cache_mb(Some(50)), 100);
        assert_eq!(
            FluxaCore::safe_dolby_vision_fallback_mode(None, Some(false), Some(true)),
            "dv8",
        );
        assert_eq!(
            FluxaCore::safe_stream_source_selection_mode(Some("unknown")),
            "manual",
        );
    }

    #[test]
    fn public_core_api_drives_player_stream_loading_effects() {
        let result = FluxaCore::player_flow_dispatch_json(
            "{}",
            r#"{"type":"loadStreamsRequested","contentType":"movie","id":"tt1","currentVideoId":"tt1","initialVideoId":null,"initialStreams":[],"initialStreamIndex":0}"#,
        )
        .expect("flow result");

        assert!(result.contains(r#""type":"loadStreams""#));
        assert!(result.contains(r#""isBuffering":true"#));
    }
}
