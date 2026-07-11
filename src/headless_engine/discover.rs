use super::helpers::{active_profile_id, normalize_error};
use super::state::GenerationKey;
use super::{EffectResultInput, HeadlessEngine};
use crate::runtime::{EffectEnvelope, EffectKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct DiscoverState {
    content_type: String,
    filters: Value,
    is_loading: bool,
    catalogs_loading: bool,
    results: Value,
    result_sources: Value,
    catalogs: Value,
    genres: Value,
    error: Value,
    generation: u64,
    paging: DiscoverPaging,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct DiscoverPaging {
    is_loading: bool,
    items: Value,
    error: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchDiscoverPagePayload {
    transport_url: Option<String>,
    content_type: String,
    catalog_id: String,
    skip: i32,
    genre: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunDiscoverPayload {
    content_type: String,
    filters: Value,
    profile_id: String,
    profile: Value,
    language: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadDiscoverCatalogFiltersPayload {
    content_type: String,
    selected_catalog_key: Option<String>,
    profile_id: String,
    profile: Value,
    language: String,
}

pub(super) fn dispatch_discover(
    engine: &mut HeadlessEngine,
    content_type: String,
    filters: Option<Value>,
    profile: Option<Value>,
    language: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Discover);
    engine.bump_generation(GenerationKey::DiscoverPaging);
    let profile_value = profile.unwrap_or_else(|| engine.state.profile.active.clone());
    let profile_id = active_profile_id(&engine.state, &profile_value);
    let filters_value = filters.unwrap_or(Value::Null);
    *engine.state.discover = DiscoverState {
        content_type: content_type.clone(),
        filters: filters_value.clone(),
        is_loading: true,
        catalogs_loading: engine.state.discover.catalogs_loading,
        results: serde_json::json!([]),
        result_sources: Value::Null,
        catalogs: engine.state.discover.catalogs.clone(),
        genres: engine.state.discover.genres.clone(),
        error: Value::Null,
        generation,
        paging: DiscoverPaging::default(),
    };
    vec![engine.effect(
        EffectKind::RunDiscover,
        generation,
        RunDiscoverPayload {
            content_type,
            filters: filters_value,
            profile_id,
            profile: profile_value,
            language: language.unwrap_or_else(|| "en".to_string()),
        },
    )]
}

pub(super) fn dispatch_catalog_filters(
    engine: &mut HeadlessEngine,
    content_type: String,
    selected_catalog_key: Option<String>,
    profile: Option<Value>,
    language: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Discover);
    let profile_value = profile.unwrap_or_else(|| engine.state.profile.active.clone());
    let profile_id = active_profile_id(&engine.state, &profile_value);
    engine.state.discover.content_type = content_type.clone();
    engine.state.discover.catalogs = serde_json::json!([]);
    engine.state.discover.catalogs_loading = true;
    vec![engine.effect(
        EffectKind::ReadDiscoverCatalogFilters,
        generation,
        ReadDiscoverCatalogFiltersPayload {
            content_type,
            selected_catalog_key,
            profile_id,
            profile: profile_value,
            language: language.unwrap_or_else(|| "en".to_string()),
        },
    )]
}

pub(super) fn dispatch_discover_page(
    engine: &mut HeadlessEngine,
    transport_url: Option<String>,
    content_type: String,
    catalog_id: String,
    skip: Option<i32>,
    genre: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::DiscoverPaging);
    engine.state.discover.paging = DiscoverPaging {
        is_loading: true,
        items: Value::Null,
        error: Value::Null,
    };
    vec![engine.effect(
        EffectKind::FetchDiscoverPage,
        generation,
        FetchDiscoverPagePayload {
            transport_url,
            content_type,
            catalog_id,
            skip: skip.unwrap_or(0).max(0),
            genre,
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
        "runDiscover" => {
            if generation == engine.state.runtime.get(GenerationKey::Discover) {
                engine.state.discover.is_loading = false;
                if result.status.is_ok() {
                    engine.state.discover.results = result
                        .value
                        .get("results")
                        .cloned()
                        .unwrap_or_else(|| result.value.clone());
                    engine.state.discover.result_sources = result
                        .value
                        .get("resultSources")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    engine.state.discover.error = Value::Null;
                } else {
                    engine.state.discover.error = normalize_error(result.error.clone());
                }
            }
        }
        "readDiscoverCatalogFilters" => {
            if generation == engine.state.runtime.get(GenerationKey::Discover) {
                engine.state.discover.catalogs_loading = false;
                if result.status.is_ok() {
                    engine.state.discover.catalogs = result
                        .value
                        .get("catalogs")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    engine.state.discover.genres = result
                        .value
                        .get("genres")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    engine.state.discover.error = Value::Null;
                } else {
                    engine.state.discover.error = normalize_error(result.error.clone());
                }
            }
        }
        "fetchDiscoverPage" => {
            if generation == engine.state.runtime.get(GenerationKey::DiscoverPaging) {
                engine.state.discover.paging.is_loading = false;
                if result.status.is_ok() {
                    engine.state.discover.paging.items = result
                        .value
                        .get("items")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    engine.state.discover.paging.error = Value::Null;
                } else {
                    engine.state.discover.paging.error = normalize_error(result.error.clone());
                }
            }
        }
        _ => {}
    }
    vec![]
}
