use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub(crate) fn is_http_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

pub(crate) fn strip_http_scheme(value: &str) -> &str {
    value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .or_else(|| value.strip_prefix("HTTP://"))
        .or_else(|| value.strip_prefix("HTTPS://"))
        .unwrap_or(value)
}

pub(crate) fn is_ipv4_like_host(value: &str) -> bool {
    let host = value
        .split('/')
        .next()
        .unwrap_or(value)
        .split(':')
        .next()
        .unwrap_or(value);
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok())
}

pub(crate) fn is_local_url(value: &str) -> bool {
    let lower = strip_http_scheme(value).to_ascii_lowercase();
    if lower.starts_with("localhost")
        || lower.starts_with("127.")
        || lower.starts_with("10.")
        || lower.starts_with("192.168.")
    {
        return true;
    }
    // 172.16.0.0/12 private range
    if let Some(rest) = lower.strip_prefix("172.") {
        if let Some(second_octet) = rest.split('.').next().and_then(|s| s.parse::<u8>().ok()) {
            if (16..=31).contains(&second_octet) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn normalize_manifest_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    let with_scheme = if lower.starts_with("stremio://") {
        format!("https://{}", &trimmed[10..])
    } else if lower.starts_with("http://") {
        if is_local_url(trimmed) {
            trimmed.to_string()
        } else {
            format!("https://{}", &trimmed[7..])
        }
    } else if lower.starts_with("https://") {
        trimmed.to_string()
    } else if lower.starts_with("127.0.0.1")
        || lower.starts_with("10.0.2.2")
        || lower.starts_with("localhost")
        || is_ipv4_like_host(trimmed)
    {
        format!("http://{trimmed}")
    } else {
        format!("https://{trimmed}")
    };

    if with_scheme.to_ascii_lowercase().ends_with("manifest.json") {
        with_scheme
    } else if with_scheme.ends_with('/') {
        format!("{with_scheme}manifest.json")
    } else {
        format!("{with_scheme}/manifest.json")
    }
}

pub(crate) fn identity(raw: &str) -> String {
    normalize_manifest_url(raw)
        .trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}

pub(crate) fn manifest_candidates(raw: &str) -> Vec<String> {
    let normalized = normalize_manifest_url(raw);
    let mut values = vec![normalized.clone()];
    if is_local_url(&normalized) && normalized.to_ascii_lowercase().starts_with("https://") {
        let fallback = format!("http://{}", &normalized[8..]);
        if !values.contains(&fallback) {
            values.push(fallback);
        }
    }
    values
}

pub(crate) fn manifest_fetch_plan_json(raw: &str) -> Option<String> {
    let normalized_transport_url = normalize_manifest_url(raw);
    if normalized_transport_url.is_empty() {
        return None;
    }
    serde_json::to_string(&json!({
        "normalizedTransportUrl": normalized_transport_url,
        "cacheKey": format!("manifest_v10_{}", normalized_transport_url),
        "candidateUrls": manifest_candidates(&normalized_transport_url)
    }))
    .ok()
}

pub(crate) fn base_url(raw: &str) -> String {
    let normalized = normalize_manifest_url(raw);
    let without_manifest = match normalized.to_ascii_lowercase().rfind("manifest.json") {
        Some(index) => normalized[..index].to_string(),
        None => normalized,
    };
    let mut base = if without_manifest.ends_with('/') {
        without_manifest
    } else {
        format!("{without_manifest}/")
    };
    let lower = base.to_ascii_lowercase();
    if (lower.contains("localhost") || lower.contains("127.0.0.1")) && lower.starts_with("https://")
    {
        base = format!("http://{}", &base[8..]);
    }
    base
}

pub(crate) fn prefer_https_asset_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") {
        if is_local_url(trimmed) {
            Some(trimmed.to_string())
        } else {
            Some(format!("https://{}", &trimmed[7..]))
        }
    } else if trimmed.starts_with("//") {
        Some(format!("https:{trimmed}"))
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'*');
        if keep {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

pub(crate) fn build_resource_url(
    raw: &str,
    resource: &str,
    content_type: &str,
    id: &str,
    extra_json: Option<&str>,
) -> String {
    let extra_path = extra_json
        .and_then(|value| serde_json::from_str::<Map<String, Value>>(value).ok())
        .map(|map| {
            map.into_iter()
                .filter_map(|(key, value)| {
                    let text = value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string());
                    if text.trim().is_empty() {
                        None
                    } else {
                        Some(format!(
                            "{}={}",
                            encode_path_segment(&key),
                            encode_path_segment(&text)
                        ))
                    }
                })
                .collect::<Vec<_>>()
                .join("&")
        })
        .filter(|value| !value.is_empty())
        .map(|value| format!("/{value}"))
        .unwrap_or_default();
    format!(
        "{}{}/{}/{}{}.json",
        base_url(raw),
        resource,
        encode_path_segment(content_type),
        encode_path_segment(id),
        extra_path
    )
}

pub(crate) fn string_array(json: &Value, key: &str) -> Vec<Value> {
    json.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .filter(|text| !text.is_empty())
                        .map(|text| Value::String(text.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn first_text(
    json: &Value,
    behavior_hints: Option<&Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        json.get(*key)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .or_else(|| {
                behavior_hints
                    .and_then(|hints| hints.get(*key))
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
            })
            .map(str::to_string)
    })
}

pub(crate) fn resolve_asset_url(asset: Option<String>, manifest_url: &str) -> Option<String> {
    let secure = prefer_https_asset_url(asset?.as_str())?;
    if is_http_url(&secure) {
        return Some(secure);
    }
    if secure.starts_with('/') {
        let base = base_url(manifest_url);
        let scheme_end = base.find("://").map(|index| index + 3)?;
        let host_end = base[scheme_end..]
            .find('/')
            .map(|index| scheme_end + index)
            .unwrap_or(base.len());
        return prefer_https_asset_url(&format!("{}{}", &base[..host_end], secure));
    }
    prefer_https_asset_url(&format!("{}{}", base_url(manifest_url), secure))
}

fn text_value<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn non_empty_array(value: Option<&Value>) -> Option<Value> {
    value
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .cloned()
        .map(Value::Array)
}

fn set_or_null(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    map.insert(
        key.to_string(),
        value.map(Value::String).unwrap_or(Value::Null),
    );
}

fn value_has_content(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

pub(crate) fn resolve_manifest_assets_json(descriptor_json: &str) -> Option<String> {
    let mut descriptor: Value = serde_json::from_str(descriptor_json).ok()?;
    let transport_url = descriptor
        .get("transportUrl")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let normalized_transport_url = normalize_manifest_url(&transport_url);
    descriptor["transportUrl"] = Value::String(normalized_transport_url.clone());

    let manifest = descriptor.get_mut("manifest")?.as_object_mut()?;
    let logo = text_value(&Value::Object(manifest.clone()), "logo").map(str::to_string);
    let background = text_value(&Value::Object(manifest.clone()), "background").map(str::to_string);
    let resolved_background = resolve_asset_url(background.clone(), &normalized_transport_url);
    let resolved_logo =
        resolve_asset_url(logo, &normalized_transport_url).or_else(|| resolved_background.clone());
    let description = manifest
        .get("description")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);

    set_or_null(manifest, "description", description);
    set_or_null(manifest, "logo", resolved_logo);
    set_or_null(manifest, "background", resolved_background);
    serde_json::to_string(&descriptor).ok()
}

pub(crate) fn merge_live_manifest_json(
    descriptor_json: &str,
    live_json: Option<&str>,
    unknown_name: &str,
) -> Option<String> {
    let Some(live_json) = live_json.filter(|value| !value.trim().is_empty()) else {
        return resolve_manifest_assets_json(descriptor_json);
    };
    let mut descriptor: Value = serde_json::from_str(descriptor_json).ok()?;
    let live: Value = serde_json::from_str(live_json).ok()?;
    let transport_url = descriptor
        .get("transportUrl")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let normalized_transport_url = normalize_manifest_url(&transport_url);
    descriptor["transportUrl"] = Value::String(normalized_transport_url.clone());

    let current_manifest_snapshot = descriptor.get("manifest")?.clone();
    let current_manifest = current_manifest_snapshot.as_object()?;
    let live_manifest = live.get("manifest")?.as_object()?;
    let manifest = descriptor.get_mut("manifest")?.as_object_mut()?;

    if let Some(id) = text_value(&Value::Object(live_manifest.clone()), "id") {
        manifest.insert("id".to_string(), Value::String(id.to_string()));
    }
    if let Some(name) = text_value(&Value::Object(live_manifest.clone()), "name")
        .filter(|name| *name != unknown_name)
    {
        manifest.insert("name".to_string(), Value::String(name.to_string()));
    }
    if let Some(description) = text_value(&Value::Object(live_manifest.clone()), "description") {
        manifest.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
    for (key, value) in live_manifest {
        if matches!(
            key.as_str(),
            "id" | "name" | "description" | "logo" | "background"
        ) {
            continue;
        }
        if matches!(
            key.as_str(),
            "resources" | "types" | "catalogs" | "idPrefixes"
        ) {
            if let Some(value) = non_empty_array(Some(value)) {
                manifest.insert(key.to_string(), value);
            }
            continue;
        }
        if value_has_content(value) {
            manifest.insert(key.to_string(), value.clone());
        }
    }

    let current_logo = current_manifest
        .get("logo")
        .and_then(Value::as_str)
        .map(str::to_string);
    let current_background = current_manifest
        .get("background")
        .and_then(Value::as_str)
        .map(str::to_string);
    let live_logo = live_manifest
        .get("logo")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let live_background = live_manifest
        .get("background")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);

    let resolved_current_logo = resolve_asset_url(current_logo, &normalized_transport_url);
    let resolved_current_background =
        resolve_asset_url(current_background, &normalized_transport_url);
    let logo = live_logo
        .or(resolved_current_logo)
        .or_else(|| live_background.clone())
        .or_else(|| resolved_current_background.clone());
    let background = live_background.or(resolved_current_background);
    set_or_null(manifest, "logo", logo);
    set_or_null(manifest, "background", background);

    serde_json::to_string(&descriptor).ok()
}

pub(crate) fn parse_catalogs(json: &Value) -> Vec<Value> {
    json.get("catalogs")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let object = item.as_object()?;
                    let mut map = object.clone();
                    let extra = object
                        .get("extra")
                        .and_then(Value::as_array)
                        .map(|extras| {
                            extras
                                .iter()
                                .filter_map(|extra| {
                                    let extra_object = extra.as_object()?;
                                    let mut map = Map::new();
                                    if let Some(name) = extra_object
                                        .get("name")
                                        .and_then(Value::as_str)
                                        .filter(|text| !text.is_empty())
                                    {
                                        map.insert(
                                            "name".to_string(),
                                            Value::String(name.to_string()),
                                        );
                                    }
                                    let options = string_array(extra, "options");
                                    if !options.is_empty() {
                                        map.insert("options".to_string(), Value::Array(options));
                                    }
                                    if let Some(is_required) =
                                        extra_object.get("isRequired").and_then(Value::as_bool)
                                    {
                                        map.insert(
                                            "isRequired".to_string(),
                                            Value::Bool(is_required),
                                        );
                                    }
                                    if let Some(options_limit) =
                                        extra_object.get("optionsLimit").and_then(Value::as_i64)
                                    {
                                        map.insert(
                                            "optionsLimit".to_string(),
                                            json!(options_limit as i32),
                                        );
                                    }
                                    Some(Value::Object(map))
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if !extra.is_empty() {
                        map.insert("extra".to_string(), Value::Array(extra));
                    }
                    Some(Value::Object(map))
                })
                .collect()
        })
        .unwrap_or_default()
}

// pub rather than pub(crate): re-exported under fuzz_targets for the `fuzz/`
// crate (see lib.rs). Not part of the supported public API otherwise.
pub fn parse_manifest(body: &str, transport_url: &str, unknown_name: &str) -> Option<String> {
    let json: Value = serde_json::from_str(body).ok()?;
    let behavior_hints = json.get("behaviorHints");
    let logo = first_text(
        &json,
        behavior_hints,
        &["logo", "icon", "iconUrl", "poster", "posterUrl"],
    );
    let background = first_text(
        &json,
        behavior_hints,
        &["background", "backgroundUrl", "backdrop", "backdropUrl"],
    );
    let description = first_text(
        &json,
        behavior_hints,
        &[
            "description",
            "shortDescription",
            "longDescription",
            "summary",
        ],
    );
    let resources = json
        .get("resources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let id_prefixes = string_array(&json, "idPrefixes");
    let mut manifest = json.as_object().cloned().unwrap_or_default();
    manifest.insert(
        "id".to_string(),
        Value::String(
            json.get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        ),
    );
    manifest.insert(
        "name".to_string(),
        Value::String(
            json.get("name")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .unwrap_or(unknown_name)
                .to_string(),
        ),
    );
    manifest.insert(
        "description".to_string(),
        description.map(Value::String).unwrap_or(Value::Null),
    );
    manifest.insert(
        "version".to_string(),
        first_text(&json, behavior_hints, &["version"])
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    manifest.insert("resources".to_string(), Value::Array(resources));
    manifest.insert(
        "types".to_string(),
        Value::Array(string_array(&json, "types")),
    );
    manifest.insert("catalogs".to_string(), Value::Array(parse_catalogs(&json)));
    manifest.insert(
        "idPrefixes".to_string(),
        if id_prefixes.is_empty() {
            Value::Null
        } else {
            Value::Array(id_prefixes)
        },
    );
    manifest.insert(
        "logo".to_string(),
        resolve_asset_url(logo, transport_url)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    manifest.insert(
        "background".to_string(),
        resolve_asset_url(background, transport_url)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    manifest.insert(
        "configurable".to_string(),
        behavior_hints
            .and_then(|hints| hints.get("configurable"))
            .and_then(Value::as_bool)
            .map(Value::Bool)
            .unwrap_or(Value::Null),
    );

    let descriptor = json!({
        "manifest": Value::Object(manifest),
        "transportUrl": transport_url
    });
    serde_json::to_string(&descriptor).ok()
}

pub(crate) fn canonical_resource_name(value: &str) -> String {
    match value.to_ascii_lowercase().trim_end_matches('s') {
        "metadata" => "meta".to_string(),
        "subtitle" => "subtitle".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn to_string_vec(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .filter(|item| !item.is_empty())
            .collect(),
        Some(Value::String(text)) if !text.is_empty() => vec![text.to_string()],
        _ => Vec::new(),
    }
}

pub(crate) fn supports_resource(
    manifest_json: &str,
    resource_name: &str,
    content_type: Option<&str>,
    id: Option<&str>,
) -> bool {
    let Ok(manifest) = serde_json::from_str::<Value>(manifest_json) else {
        return false;
    };
    let expected = canonical_resource_name(resource_name);
    let manifest_types = to_string_vec(manifest.get("types"));
    let manifest_prefixes = to_string_vec(manifest.get("idPrefixes"));
    manifest
        .get("resources")
        .and_then(Value::as_array)
        .map(|resources| {
            resources.iter().any(|resource| {
                let (name, types, prefixes) = match resource {
                    Value::String(name) => (
                        name.as_str(),
                        manifest_types.clone(),
                        manifest_prefixes.clone(),
                    ),
                    Value::Object(map) => {
                        let Some(name) = map.get("name").and_then(Value::as_str) else {
                            return false;
                        };
                        let types = to_string_vec(map.get("types"))
                            .into_iter()
                            .chain(to_string_vec(map.get("type")))
                            .collect::<Vec<_>>();
                        let prefixes = to_string_vec(map.get("idPrefixes"))
                            .into_iter()
                            .chain(to_string_vec(map.get("idPrefix")))
                            .collect::<Vec<_>>();
                        (
                            name,
                            if types.is_empty() {
                                manifest_types.clone()
                            } else {
                                types
                            },
                            if prefixes.is_empty() {
                                manifest_prefixes.clone()
                            } else {
                                prefixes
                            },
                        )
                    }
                    _ => return false,
                };
                if canonical_resource_name(name) != expected {
                    return false;
                }
                if let Some(content_type) = content_type {
                    if !types.is_empty()
                        && !types
                            .iter()
                            .any(|item| item.eq_ignore_ascii_case(content_type))
                    {
                        return false;
                    }
                }
                if let Some(id) = id {
                    if canonical_resource_name(name) != "catalog"
                        && !prefixes.is_empty()
                        && !prefixes.iter().any(|prefix| id.starts_with(prefix))
                    {
                        return false;
                    }
                }
                true
            })
        })
        .unwrap_or(false)
}

pub(crate) fn catalog_supports_extra(catalog_json: &str, extra_name: &str) -> bool {
    let Ok(catalog) = serde_json::from_str::<Value>(catalog_json) else {
        return false;
    };
    catalog
        .get("extraSupported")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .any(|item| item.eq_ignore_ascii_case(extra_name))
        })
        || catalog
            .get("extra")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.eq_ignore_ascii_case(extra_name))
                })
            })
}

pub(crate) fn catalog_requires_extra(catalog_json: &str, extra_name: &str) -> bool {
    let Ok(catalog) = serde_json::from_str::<Value>(catalog_json) else {
        return false;
    };
    catalog
        .get("extra")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.eq_ignore_ascii_case(extra_name))
                    && item.get("isRequired").and_then(Value::as_bool) == Some(true)
            })
        })
}

pub(crate) fn catalog_has_required_extra_except(
    catalog_json: &str,
    allowed_names_json: &str,
) -> bool {
    let Ok(catalog) = serde_json::from_str::<Value>(catalog_json) else {
        return false;
    };
    let allowed_names = serde_json::from_str::<Vec<String>>(allowed_names_json)
        .unwrap_or_default()
        .into_iter()
        .collect::<HashSet<_>>();
    catalog
        .get("extra")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("isRequired").and_then(Value::as_bool) == Some(true)
                    && item
                        .get("name")
                        .and_then(Value::as_str)
                        .map(|name| !allowed_names.contains(&name.to_ascii_lowercase()))
                        .unwrap_or(true)
            })
        })
}

fn meta_link_kind(category: &str) -> Option<&'static str> {
    let category = category.to_lowercase();
    if ["cast", "actor", "actors", "starring"]
        .iter()
        .any(|key| category.contains(key))
    {
        Some("cast")
    } else if category.contains("director") {
        Some("director")
    } else {
        None
    }
}

pub(crate) fn classify_meta_links_json(links_json: &str) -> Option<String> {
    let links: Vec<Value> = serde_json::from_str(links_json).ok()?;
    let mut cast: Vec<Value> = Vec::new();
    let mut directors: Vec<Value> = Vec::new();
    for link in links {
        let category = link.get("category").and_then(Value::as_str).unwrap_or("");
        match meta_link_kind(category) {
            Some("cast") => cast.push(link),
            Some("director") => directors.push(link),
            _ => {}
        }
    }
    serde_json::to_string(&json!({ "cast": cast, "directors": directors })).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        build_resource_url, merge_live_manifest_json, parse_manifest, resolve_manifest_assets_json,
        supports_resource,
    };
    use serde_json::{json, Value};

    #[test]
    fn resolve_manifest_assets_normalizes_transport_and_relative_assets() {
        let descriptor = r#"{
            "transportUrl":"addon.example/root/manifest.json",
            "manifest":{
                "id":"addon",
                "name":"Addon",
                "description":"",
                "resources":[],
                "types":[],
                "catalogs":[],
                "logo":"logo.png",
                "background":"/bg.jpg"
            }
        }"#;
        let resolved = resolve_manifest_assets_json(descriptor)
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("resolved descriptor");

        assert_eq!(
            resolved.get("transportUrl").and_then(Value::as_str),
            Some("https://addon.example/root/manifest.json")
        );
        assert_eq!(
            resolved
                .get("manifest")
                .and_then(|manifest| manifest.get("description")),
            Some(&Value::Null)
        );
        assert_eq!(
            resolved
                .get("manifest")
                .and_then(|manifest| manifest.get("logo"))
                .and_then(Value::as_str),
            Some("https://addon.example/root/logo.png")
        );
        assert_eq!(
            resolved
                .get("manifest")
                .and_then(|manifest| manifest.get("background"))
                .and_then(Value::as_str),
            Some("https://addon.example/bg.jpg")
        );
    }

    #[test]
    fn manifest_fetch_plan_owns_cache_key_and_candidates() {
        let plan = super::manifest_fetch_plan_json("127.0.0.1:7000")
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("manifest fetch plan");

        assert_eq!(
            plan.get("normalizedTransportUrl").and_then(Value::as_str),
            Some("http://127.0.0.1:7000/manifest.json")
        );
        assert_eq!(
            plan.get("cacheKey").and_then(Value::as_str),
            Some("manifest_v10_http://127.0.0.1:7000/manifest.json")
        );
        assert_eq!(
            plan.get("candidateUrls")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(Value::as_str),
            Some("http://127.0.0.1:7000/manifest.json")
        );
    }

    #[test]
    fn merge_live_manifest_keeps_current_fields_when_live_is_empty() {
        let current = r#"{
            "transportUrl":"https://addon.example/manifest.json",
            "manifest":{
                "id":"old",
                "name":"Old",
                "description":"Current",
                "version":"1.0",
                "resources":["stream"],
                "types":["movie"],
                "catalogs":[{"type":"movie","id":"old"}],
                "logo":"logo.png",
                "background":"bg.jpg",
                "configurable":false
            }
        }"#;
        let live = r#"{
            "transportUrl":"https://addon.example/manifest.json",
            "manifest":{
                "id":"new",
                "name":"Unknown",
                "description":"",
                "version":"2.0",
                "resources":[],
                "types":[],
                "catalogs":[],
                "logo":null,
                "background":"live-bg.jpg",
                "configurable":true
            }
        }"#;
        let merged = merge_live_manifest_json(current, Some(live), "Unknown")
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("merged descriptor");
        let manifest = merged.get("manifest").expect("manifest");

        assert_eq!(manifest.get("id").and_then(Value::as_str), Some("new"));
        assert_eq!(manifest.get("name").and_then(Value::as_str), Some("Old"));
        assert_eq!(
            manifest.get("description").and_then(Value::as_str),
            Some("Current")
        );
        assert_eq!(manifest.get("version").and_then(Value::as_str), Some("2.0"));
        assert_eq!(
            manifest
                .get("resources")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            manifest.get("logo").and_then(Value::as_str),
            Some("https://addon.example/logo.png")
        );
        assert_eq!(
            manifest.get("background").and_then(Value::as_str),
            Some("live-bg.jpg")
        );
        assert_eq!(
            manifest.get("configurable").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn parse_manifest_preserves_stremio_manifest_fields() {
        let parsed = parse_manifest(
            r#"{
                "id":"org.example.full",
                "version":"1.2.3",
                "name":"Full",
                "description":"All fields",
                "resources":["catalog",{"name":"stream","types":["movie"],"idPrefixes":["tt"]}],
                "types":["movie","series"],
                "idPrefixes":["tt"],
                "catalogs":[{"type":"movie","id":"top","name":"Top","extra":[{"name":"genre","isRequired":false,"options":["Drama"],"optionsLimit":2}],"extraSupported":["search"],"extraRequired":["genre"]}],
                "addonCatalogs":[{"type":"addon","id":"community","name":"Community"}],
                "config":[{"key":"token","type":"password","default":"x","title":"Token","required":true}],
                "background":"/bg.jpg",
                "logo":"logo.png",
                "contactEmail":"ops@example.com",
                "behaviorHints":{"adult":true,"p2p":true,"configurable":true,"configurationRequired":true}
            }"#,
            "https://addon.example/root/manifest.json",
            "Unknown",
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("parsed manifest");
        let manifest = parsed.get("manifest").expect("manifest");

        assert_eq!(
            manifest["addonCatalogs"][0]["id"].as_str(),
            Some("community")
        );
        assert_eq!(manifest["config"][0]["key"].as_str(), Some("token"));
        assert_eq!(manifest["contactEmail"].as_str(), Some("ops@example.com"));
        assert_eq!(
            manifest["behaviorHints"]["configurationRequired"].as_bool(),
            Some(true)
        );
        assert_eq!(
            manifest["catalogs"][0]["extraRequired"][0].as_str(),
            Some("genre")
        );
        assert_eq!(
            manifest["logo"].as_str(),
            Some("https://addon.example/root/logo.png")
        );
        assert_eq!(
            manifest["background"].as_str(),
            Some("https://addon.example/bg.jpg")
        );
    }

    #[test]
    fn merge_live_manifest_preserves_new_live_manifest_fields() {
        let current = r#"{
            "transportUrl":"https://addon.example/manifest.json",
            "manifest":{"id":"old","name":"Old","description":"Current","resources":["stream"],"types":["movie"],"catalogs":[]}
        }"#;
        let live = r#"{
            "manifest":{
                "id":"old",
                "name":"Live",
                "description":"Live description",
                "resources":["stream"],
                "types":["movie"],
                "catalogs":[],
                "addonCatalogs":[{"type":"addon","id":"community","name":"Community"}],
                "config":[{"key":"token","type":"password"}],
                "contactEmail":"ops@example.com",
                "behaviorHints":{"configurable":true,"configurationRequired":true}
            }
        }"#;
        let merged = merge_live_manifest_json(current, Some(live), "Unknown")
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("merged manifest");
        let manifest = merged.get("manifest").expect("manifest");

        assert_eq!(
            manifest["addonCatalogs"][0]["id"].as_str(),
            Some("community")
        );
        assert_eq!(manifest["config"][0]["key"].as_str(), Some("token"));
        assert_eq!(manifest["contactEmail"].as_str(), Some("ops@example.com"));
        assert_eq!(
            manifest["behaviorHints"]["configurationRequired"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn build_resource_url_appends_extra_path_segment_and_omits_blank_values() {
        assert_eq!(
            build_resource_url(
                "https://addon.example/manifest.json",
                "stream",
                "movie",
                "tt123",
                None
            ),
            "https://addon.example/stream/movie/tt123.json"
        );
        assert_eq!(
            build_resource_url(
                "https://addon.example/manifest.json",
                "catalog",
                "movie",
                "top",
                Some(r#"{"genre":"action"}"#),
            ),
            "https://addon.example/catalog/movie/top/genre=action.json"
        );
        // A blank extra value contributes nothing — same shape as no extra at all.
        assert_eq!(
            build_resource_url(
                "https://addon.example/manifest.json",
                "stream",
                "movie",
                "tt123",
                Some(r#"{"genre":""}"#),
            ),
            "https://addon.example/stream/movie/tt123.json"
        );
    }

    #[test]
    fn supports_resource_gates_on_content_type_but_catalog_bypasses_id_prefix() {
        let manifest = json!({
            "resources": ["stream"],
            "types": ["movie"],
            "idPrefixes": ["tt"],
        });
        assert!(supports_resource(
            &manifest.to_string(),
            "streams",
            Some("movie"),
            Some("tt123")
        ));
        // Wrong content type for this manifest's declared types.
        assert!(!supports_resource(
            &manifest.to_string(),
            "stream",
            Some("series"),
            Some("tt123")
        ));

        let catalog_manifest = json!({
            "resources": [{ "name": "catalog", "types": ["movie"], "idPrefixes": ["tt"] }],
        });
        // catalog resources never gate on id prefix, even though one is declared.
        assert!(supports_resource(
            &catalog_manifest.to_string(),
            "catalog",
            Some("movie"),
            Some("does-not-match-prefix")
        ));

        assert!(!supports_resource("not json", "stream", None, None));
    }
}