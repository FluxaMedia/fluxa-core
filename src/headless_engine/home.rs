use super::helpers::{active_profile_id, error_code, normalize_error};
use super::player;
use super::state::GenerationKey;
use super::{EffectResultInput, HeadlessEngine};
use crate::runtime::{EffectEnvelope, EffectKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct HomeState {
    is_loading: bool,
    categories: Value,
    continue_watching: Value,
    user_addons: Value,
    metadata_feeds: Value,
    billboard: Value,
    watchlist: Value,
    is_direct_loading: bool,
    active_profile: Value,
    external_continue_watching: Value,
    paging: HomePaging,
    error: Value,
    generation: u64,
}

impl Default for HomeState {
    fn default() -> Self {
        Self {
            is_loading: false,
            categories: serde_json::json!([]),
            continue_watching: serde_json::json!([]),
            user_addons: serde_json::json!([]),
            metadata_feeds: serde_json::json!([]),
            billboard: Value::Null,
            watchlist: serde_json::json!([]),
            is_direct_loading: false,
            active_profile: Value::Null,
            external_continue_watching: serde_json::json!([]),
            paging: HomePaging::default(),
            error: Value::Null,
            generation: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct HomePaging {
    category_id: String,
    is_loading: bool,
    items: Value,
    error: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadHomeBootstrapPayload {
    profile_id: String,
    profile: Value,
    language: String,
    force: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareDirectPlaybackPayload {
    meta: Value,
    language: String,
    profile: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchCatalogPagePayload {
    category_id: String,
    transport_url: Option<String>,
    content_type: String,
    catalog_id: String,
    skip: i32,
    genre: Option<String>,
    search: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RefreshContinueWatchingPayload {
    profile_id: String,
    profile: Value,
    language: String,
}

pub(super) fn dispatch_refresh_continue_watching(
    engine: &mut HeadlessEngine,
    profile: Option<Value>,
    language: Option<String>,
) -> Vec<EffectEnvelope> {
    let profile_value = profile.unwrap_or_else(|| engine.state.profile.active.clone());
    let profile_id = active_profile_id(&engine.state, &profile_value);
    let generation = engine.state.runtime.get(GenerationKey::Home);
    vec![engine.effect(
        EffectKind::RefreshContinueWatching,
        generation,
        RefreshContinueWatchingPayload {
            profile_id,
            profile: profile_value,
            language: language.unwrap_or_else(|| "en".to_string()),
        },
    )]
}

pub(super) fn remove_from_continue_watching(engine: &mut HeadlessEngine, dropped_id: &str) {
    if let Some(items) = engine.state.home.continue_watching.as_array_mut() {
        items.retain(|item| item.get("id").and_then(Value::as_str) != Some(dropped_id));
    }
}

pub(super) fn mirror_active_profile(engine: &mut HeadlessEngine, profile: Value) {
    engine.state.home.active_profile = profile;
}

pub(super) fn set_user_addons(engine: &mut HeadlessEngine, addons: Value) {
    engine.state.home.user_addons = addons;
}

pub(super) fn set_external_continue_watching(engine: &mut HeadlessEngine, items: Value) {
    engine.state.home.external_continue_watching = items;
}

pub(super) fn dispatch_load(
    engine: &mut HeadlessEngine,
    profile: Option<Value>,
    language: Option<String>,
    force: Option<bool>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Home);
    let profile_value = profile.unwrap_or_else(|| engine.state.profile.active.clone());
    let profile_id = active_profile_id(&engine.state, &profile_value);
    engine.state.home = HomeState {
        is_loading: true,
        generation,
        ..HomeState::default()
    };
    vec![engine.effect(
        EffectKind::ReadHomeBootstrap,
        generation,
        ReadHomeBootstrapPayload {
            profile_id,
            profile: profile_value,
            language: language.unwrap_or_else(|| "en".to_string()),
            force: force.unwrap_or(false),
        },
    )]
}

pub(super) fn dispatch_direct_playback(
    engine: &mut HeadlessEngine,
    meta: Value,
    language: Option<String>,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::PlaybackPrep);
    engine.state.home.is_direct_loading = true;
    vec![engine.effect(
        EffectKind::PrepareDirectPlayback,
        generation,
        PrepareDirectPlaybackPayload {
            meta,
            language: language.unwrap_or_else(|| "en".to_string()),
            profile: profile.unwrap_or(Value::Null),
        },
    )]
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_catalog_page(
    engine: &mut HeadlessEngine,
    category_id: String,
    transport_url: Option<String>,
    content_type: String,
    catalog_id: String,
    skip: Option<i32>,
    genre: Option<String>,
    search: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Home);
    engine.state.home.paging = HomePaging {
        category_id: category_id.clone(),
        is_loading: true,
        items: Value::Null,
        error: Value::Null,
    };
    let skip = skip.unwrap_or(0).max(0);
    vec![engine.effect(
        EffectKind::FetchCatalogPage,
        generation,
        FetchCatalogPagePayload {
            category_id,
            transport_url,
            content_type,
            catalog_id,
            skip,
            genre,
            search,
        },
    )]
}

pub(super) fn complete(
    engine: &mut HeadlessEngine,
    effect_type: &str,
    generation: u64,
    result: &EffectResultInput,
) -> Vec<EffectEnvelope> {
    match effect_type {
        "refreshContinueWatching" => {
            if result.status.is_ok() {
                if let Some(cw) = result.value.get("continueWatching") {
                    engine.state.home.continue_watching = cw.clone();
                }
            }
        }
        "readHomeBootstrap" => {
            if generation == engine.state.runtime.get(GenerationKey::Home) {
                engine.state.home.is_loading = false;
                if result.status.is_ok() {
                    engine.state.home.categories = result
                        .value
                        .get("categories")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    engine.state.home.continue_watching = result
                        .value
                        .get("continueWatching")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    engine.state.home.watchlist = result
                        .value
                        .get("watchlist")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    engine.state.home.user_addons = result
                        .value
                        .get("userAddons")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    engine.state.home.metadata_feeds = result
                        .value
                        .get("metadataFeeds")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    engine.state.home.billboard = result
                        .value
                        .get("billboard")
                        .cloned()
                        .unwrap_or(Value::Null);
                    engine.state.home.error = Value::Null;
                } else {
                    engine.state.home.error = normalize_error(result.error.clone());
                }
            }
        }
        "prepareDirectPlayback" => {
            if generation == engine.state.runtime.get(GenerationKey::PlaybackPrep) {
                engine.state.home.is_direct_loading = false;
                if result.status.is_ok() {
                    player::complete_direct_playback(engine, result.value.clone(), Value::Null);
                } else {
                    player::complete_direct_playback(
                        engine,
                        Value::Null,
                        Value::String(error_code(&result.error)),
                    );
                }
            }
        }
        "fetchCatalogPage" => {
            if generation == engine.state.runtime.get(GenerationKey::Home) {
                engine.state.home.paging.is_loading = false;
                if result.status.is_ok() {
                    engine.state.home.paging.items = result
                        .value
                        .get("items")
                        .cloned()
                        .unwrap_or_else(|| result.value.clone());
                    engine.state.home.paging.error = Value::Null;
                } else {
                    engine.state.home.paging.error = normalize_error(result.error.clone());
                }
            }
        }
        _ => {}
    }
    vec![]
}
