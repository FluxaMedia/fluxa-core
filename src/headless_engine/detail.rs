use super::helpers::{
    active_profile_id, current_generation, normalize_error, normalize_meta_trailers,
    value_array_is_empty, visible_streams,
};
use super::{EffectResultInput, HeadlessEngine};
use crate::runtime::{EffectEnvelope, EffectKind};
use serde_json::{json, Value};

pub(super) fn dispatch_load(
    engine: &mut HeadlessEngine,
    content_type: String,
    id: String,
    language: Option<String>,
    source_addon_transport_url: Option<String>,
    source_addon_catalog_type: Option<String>,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation("detailGeneration");
    engine.state["detail"] = json!({
        "contentType": content_type,
        "id": id,
        "language": language.clone().unwrap_or_else(|| "en".to_string()),
        "profile": profile.clone().unwrap_or(Value::Null),
        "isLoading": true,
        "isLoadingStreams": false,
        "meta": Value::Null,
        "streams": [],
        "visibleStreams": [],
        "selectedAddon": Value::Null,
        "availableAddons": [],
        "loadingAddonNames": [],
        "seasonEpisodes": [],
        "savedPlayback": Value::Null,
        "watchedVideoIds": [],
        "similarItems": [],
        "trailers": [],
        "error": Value::Null,
        "generation": generation
    });
    vec![
        engine.effect(
            EffectKind::FetchMetaDetail,
            generation,
            json!({
                "contentType": content_type,
                "id": id,
                "language": language.unwrap_or_else(|| "en".to_string()),
                "sourceAddonTransportUrl": source_addon_transport_url.unwrap_or_default(),
                "sourceAddonCatalogType": source_addon_catalog_type.unwrap_or_default(),
                "profile": profile.unwrap_or(Value::Null)
            }),
        ),
        engine.effect(
            EffectKind::ReadPlaybackProgress,
            generation,
            json!({ "id": engine.state["detail"]["id"].clone() }),
        ),
    ]
}

pub(super) fn dispatch_local_state(
    engine: &mut HeadlessEngine,
    primary_id: String,
    fallback_id: Option<String>,
    content_type: String,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let generation = current_generation(&engine.state, "detailGeneration")
        .max(engine.bump_generation("detailGeneration"));
    vec![engine.effect(
        EffectKind::ReadDetailLocalState,
        generation,
        json!({
            "primaryId": primary_id,
            "fallbackId": fallback_id,
            "contentType": content_type,
            "profile": profile.unwrap_or(Value::Null)
        }),
    )]
}

pub(super) fn dispatch_secondary(
    engine: &mut HeadlessEngine,
    content_type: String,
    id: String,
    language: Option<String>,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let generation = current_generation(&engine.state, "detailGeneration")
        .max(engine.bump_generation("detailGeneration"));
    vec![engine.effect(
        EffectKind::FetchDetailSecondary,
        generation,
        json!({
            "contentType": content_type,
            "id": id,
            "language": language.unwrap_or_else(|| "en".to_string()),
            "profile": profile.unwrap_or(Value::Null)
        }),
    )]
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_prefetch(
    engine: &mut HeadlessEngine,
    content_type: String,
    id: String,
    stream_lookup_id: String,
    title: Option<String>,
    original_name: Option<String>,
    year: Option<i32>,
    language: Option<String>,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let generation = current_generation(&engine.state, "detailGeneration");
    vec![engine.effect(
        EffectKind::PrefetchDetailStreams,
        generation,
        json!({
            "contentType": content_type,
            "id": id,
            "streamLookupId": stream_lookup_id,
            "title": title.unwrap_or_default(),
            "originalName": original_name,
            "year": year,
            "language": language.unwrap_or_else(|| "en".to_string()),
            "profile": profile.unwrap_or(Value::Null)
        }),
    )]
}

pub(super) fn dispatch_streams(
    engine: &mut HeadlessEngine,
    content_type: String,
    request_ids: Vec<String>,
    detail: Option<Value>,
    season_episodes: Option<Vec<Value>>,
    language: Option<String>,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation("detailStreamsGeneration");
    engine.state["detail"]["isLoadingStreams"] = json!(true);
    engine.state["detail"]["streams"] = json!([]);
    engine.state["detail"]["visibleStreams"] = json!([]);
    engine.state["detail"]["selectedAddon"] = Value::Null;
    engine.state["detail"]["availableAddons"] = json!([]);
    engine.state["detail"]["loadingAddonNames"] = json!([]);
    vec![engine.effect(
        EffectKind::FetchDetailStreams,
        generation,
        json!({
            "contentType": content_type,
            "requestIds": request_ids,
            "detail": detail.unwrap_or(Value::Null),
            "seasonEpisodes": season_episodes.unwrap_or_default(),
            "language": language.unwrap_or_else(|| "en".to_string()),
            "profile": profile.unwrap_or(Value::Null)
        }),
    )]
}

pub(super) fn dispatch_streams_appended(
    engine: &mut HeadlessEngine,
    streams: Vec<Value>,
    available_addons: Vec<String>,
) -> Vec<EffectEnvelope> {
    if !engine.state["detail"]["isLoadingStreams"]
        .as_bool()
        .unwrap_or(false)
    {
        return vec![];
    }
    let mut merged: Vec<Value> = engine.state["detail"]["streams"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    merged.extend(streams);
    engine.state["detail"]["streams"] = json!(merged);
    engine.state["detail"]["visibleStreams"] = visible_streams(
        &engine.state["detail"]["streams"],
        engine.state["detail"]["selectedAddon"].as_str(),
    );
    let mut all_addons: Vec<String> = engine.state["detail"]["availableAddons"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for addon in available_addons {
        if !all_addons.contains(&addon) {
            all_addons.push(addon);
        }
    }
    engine.state["detail"]["availableAddons"] = json!(all_addons);
    engine.state["detail"]["hasStreamProviders"] = json!(!value_array_is_empty(
        &engine.state["detail"]["streams"]
    ));
    vec![]
}

pub(super) fn dispatch_selected_addon_changed(
    engine: &mut HeadlessEngine,
    addon: Option<String>,
) -> Vec<EffectEnvelope> {
    let selected = addon.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });
    engine.state["detail"]["selectedAddon"] =
        selected.as_ref().map(|value| json!(value)).unwrap_or(Value::Null);
    engine.state["detail"]["visibleStreams"] =
        visible_streams(&engine.state["detail"]["streams"], selected.as_deref());
    vec![]
}

pub(super) fn dispatch_meta_detail(
    engine: &mut HeadlessEngine,
    content_type: String,
    id: String,
    language: Option<String>,
    profile: Option<Value>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation("lookupGeneration");
    vec![engine.effect(
        EffectKind::FetchMetaDetailLookup,
        generation,
        json!({
            "contentType": content_type,
            "id": id,
            "language": language.unwrap_or_else(|| "en".to_string()),
            "profile": profile.unwrap_or(Value::Null)
        }),
    )]
}

pub(super) fn dispatch_season(
    engine: &mut HeadlessEngine,
    series_id: String,
    season: i32,
    profile: Option<Value>,
    language: Option<String>,
) -> Vec<EffectEnvelope> {
    let generation = current_generation(&engine.state, "detailGeneration")
        .max(engine.bump_generation("detailGeneration"));
    let profile_value = profile.unwrap_or_else(|| engine.state["profile"]["active"].clone());
    engine.state["detail"]["seasonLoading"] = json!(season);
    vec![engine.effect(
        EffectKind::FetchSeasonEpisodes,
        generation,
        json!({
            "seriesId": series_id,
            "season": season.max(0),
            "profileId": active_profile_id(&engine.state, &profile_value),
            "profile": profile_value,
            "language": language.unwrap_or_else(|| "en".to_string())
        }),
    )]
}

pub(super) fn complete(
    engine: &mut HeadlessEngine,
    effect_type: &str,
    generation: u64,
    result: &EffectResultInput,
) -> Vec<EffectEnvelope> {
    match effect_type {
        "fetchMetaDetail" => {
            if generation == current_generation(&engine.state, "detailGeneration") {
                if result.status == "ok" {
                    engine.state["detail"]["trailers"] = normalize_meta_trailers(&result.value);
                    engine.state["detail"]["meta"] = result.value.clone();
                    engine.state["detail"]["isLoading"] = json!(false);
                    engine.state["detail"]["error"] = Value::Null;
                } else {
                    engine.state["detail"]["isLoading"] = json!(false);
                    engine.state["detail"]["error"] = normalize_error(result.error.clone());
                }
            }
        }
        "readPlaybackProgress" => {
            if generation == current_generation(&engine.state, "detailGeneration") {
                engine.state["detail"]["savedPlayback"] = if result.status == "ok" {
                    result.value.clone()
                } else {
                    Value::Null
                };
            }
        }
        "readDetailLocalState" => {
            if generation == current_generation(&engine.state, "detailGeneration") {
                if result.status == "ok" {
                    engine.state["detail"]["savedPlayback"] =
                        result.value.get("savedPlayback").cloned().unwrap_or(Value::Null);
                    engine.state["detail"]["localWatchedVideoIds"] = result
                        .value
                        .get("localWatchedVideoIds")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    engine.state["detail"]["isInWatchlist"] = result
                        .value
                        .get("isInWatchlist")
                        .cloned()
                        .unwrap_or_else(|| json!(false));
                    engine.state["detail"]["feedback"] =
                        result.value.get("feedback").cloned().unwrap_or(Value::Null);
                    engine.state["detail"]["hasStreamProviders"] = result
                        .value
                        .get("hasStreamProviders")
                        .cloned()
                        .unwrap_or_else(|| json!(false));
                    engine.state["detail"]["userAddons"] = result
                        .value
                        .get("userAddons")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                } else {
                    engine.state["detail"]["error"] = normalize_error(result.error.clone());
                }
            }
        }
        "fetchDetailSecondary" => {
            if generation == current_generation(&engine.state, "detailGeneration") {
                if result.status == "ok" {
                    engine.state["detail"]["watchedVideoIds"] = result
                        .value
                        .get("watchedVideoIds")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    engine.state["detail"]["similarItems"] = result
                        .value
                        .get("similarItems")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    if value_array_is_empty(&engine.state["detail"]["trailers"]) {
                        engine.state["detail"]["trailers"] = result
                            .value
                            .get("trailers")
                            .cloned()
                            .unwrap_or_else(|| json!([]));
                    }
                } else {
                    engine.state["detail"]["error"] = normalize_error(result.error.clone());
                }
            }
        }
        "prefetchDetailStreams" => {
            if generation == current_generation(&engine.state, "detailGeneration") {
                if result.status == "ok" {
                    engine.state["detail"]["lastPrefetch"] = result.value.clone();
                } else {
                    engine.state["detail"]["lastPrefetchError"] =
                        normalize_error(result.error.clone());
                }
            }
        }
        "fetchDetailStreams" => {
            if generation == current_generation(&engine.state, "detailStreamsGeneration") {
                engine.state["detail"]["isLoadingStreams"] = json!(false);
                if result.status == "ok" {
                    engine.state["detail"]["streams"] = result
                        .value
                        .get("streams")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    engine.state["detail"]["selectedAddon"] = Value::Null;
                    engine.state["detail"]["visibleStreams"] =
                        engine.state["detail"]["streams"].clone();
                    engine.state["detail"]["availableAddons"] = result
                        .value
                        .get("availableAddons")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    engine.state["detail"]["loadingAddonNames"] = json!([]);
                    engine.state["detail"]["resolvedRequestId"] = result
                        .value
                        .get("resolvedRequestId")
                        .cloned()
                        .unwrap_or(Value::Null);
                    engine.state["detail"]["hasStreamProviders"] = result
                        .value
                        .get("hasStreamProviders")
                        .cloned()
                        .unwrap_or_else(|| json!(false));
                    engine.state["detail"]["streamsError"] = Value::Null;
                } else {
                    engine.state["detail"]["streamsError"] = normalize_error(result.error.clone());
                    engine.state["detail"]["loadingAddonNames"] = json!([]);
                }
            }
        }
        "fetchMetaDetailLookup" => {
            if generation == current_generation(&engine.state, "lookupGeneration") {
                if result.status == "ok" {
                    engine.state["lookup"]["trailers"] = normalize_meta_trailers(&result.value);
                    engine.state["lookup"]["metaDetail"] = result.value.clone();
                    engine.state["lookup"]["error"] = Value::Null;
                } else {
                    engine.state["lookup"]["metaDetail"] = Value::Null;
                    engine.state["lookup"]["trailers"] = json!([]);
                    engine.state["lookup"]["error"] = normalize_error(result.error.clone());
                }
            }
        }
        "fetchSeasonEpisodes" => {
            if generation == current_generation(&engine.state, "detailGeneration") {
                engine.state["detail"]["seasonLoading"] = Value::Null;
                if result.status == "ok" {
                    engine.state["detail"]["seasonEpisodes"] = result
                        .value
                        .get("episodes")
                        .cloned()
                        .unwrap_or_else(|| result.value.clone());
                    engine.state["detail"]["error"] = Value::Null;
                } else {
                    engine.state["detail"]["error"] = normalize_error(result.error.clone());
                }
            }
        }
        _ => {}
    }
    vec![]
}
