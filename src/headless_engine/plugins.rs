use super::helpers::{normalize_error, upsert_by_key};
use super::state::GenerationKey;
use super::{EffectResultInput, HeadlessEngine};
use crate::addon_store::normalize_plugin_repository_url;
use crate::runtime::{EffectEnvelope, EffectKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct PluginsState {
    repositories: Value,
    scrapers: Value,
    adding_repository_url: Value,
    error: Value,
}

impl Default for PluginsState {
    fn default() -> Self {
        Self {
            repositories: json!([]),
            scrapers: json!([]),
            adding_repository_url: Value::Null,
            error: Value::Null,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchPluginManifestPayload {
    manifest_url: String,
}

pub(super) fn dispatch_add_repository(
    engine: &mut HeadlessEngine,
    manifest_url: String,
) -> Vec<EffectEnvelope> {
    let manifest_url = normalize_plugin_repository_url(&manifest_url);
    let generation = engine.bump_generation(GenerationKey::Plugins);
    engine.state.plugins.adding_repository_url = Value::String(manifest_url.clone());
    engine.state.plugins.error = Value::Null;
    vec![engine.effect(
        EffectKind::FetchPluginManifest,
        generation,
        FetchPluginManifestPayload { manifest_url },
    )]
}

pub(super) fn dispatch_remove_repository(
    engine: &mut HeadlessEngine,
    manifest_url: String,
) -> Vec<EffectEnvelope> {
    let manifest_url = normalize_plugin_repository_url(&manifest_url);
    if let Some(items) = engine.state.plugins.repositories.as_array_mut() {
        items.retain(|repo| repo["manifestUrl"].as_str() != Some(manifest_url.as_str()));
    }
    if let Some(items) = engine.state.plugins.scrapers.as_array_mut() {
        items.retain(|scraper| scraper["repositoryUrl"].as_str() != Some(manifest_url.as_str()));
    }
    vec![]
}

pub(super) fn dispatch_toggle_scraper(
    engine: &mut HeadlessEngine,
    scraper_id: String,
    enabled: bool,
) -> Vec<EffectEnvelope> {
    if let Some(items) = engine.state.plugins.scrapers.as_array_mut() {
        if let Some(scraper) = items
            .iter_mut()
            .find(|scraper| scraper["id"].as_str() == Some(scraper_id.as_str()))
        {
            scraper["enabled"] = Value::Bool(enabled);
        }
    }
    vec![]
}

pub(super) fn complete(
    engine: &mut HeadlessEngine,
    generation: u64,
    result: &EffectResultInput,
) -> Vec<EffectEnvelope> {
    if generation != engine.state.runtime.get(GenerationKey::Plugins) {
        return vec![];
    }
    engine.state.plugins.adding_repository_url = Value::Null;

    if !result.status.is_ok() {
        engine.state.plugins.error = normalize_error(result.error.clone());
        return vec![];
    }

    let manifest_url = result.value["manifestUrl"].as_str().unwrap_or_default();
    let manifest = &result.value["manifest"];
    if manifest_url.is_empty() || !manifest.is_object() {
        engine.state.plugins.error = normalize_error(Value::Null);
        return vec![];
    }

    let scrapers = manifest["scrapers"].as_array().cloned().unwrap_or_default();
    let repository_entry = json!({
        "manifestUrl": manifest_url,
        "name": manifest["name"],
        "description": manifest["description"],
        "version": manifest["version"],
        "scraperCount": scrapers.len(),
    });
    upsert_by_key(
        &mut engine.state.plugins.repositories,
        "manifestUrl",
        manifest_url,
        repository_entry,
    );

    if let Some(items) = engine.state.plugins.scrapers.as_array_mut() {
        items.retain(|scraper| scraper["repositoryUrl"].as_str() != Some(manifest_url));
    }
    for mut scraper in scrapers {
        scraper["repositoryUrl"] = Value::String(manifest_url.to_string());
        if let Some(items) = engine.state.plugins.scrapers.as_array_mut() {
            items.push(scraper);
        }
    }
    engine.state.plugins.error = Value::Null;
    vec![]
}
