use crate::addon_protocol;
use serde_json::{json, Value};

fn resource_payload(resource: &str, root: &Value) -> Option<Value> {
    match resource {
        "stream" | "streams" => root.get("streams").cloned(),
        "catalog" | "metas" => root.get("metas").cloned(),
        "meta" => root.get("meta").cloned(),
        "subtitles" | "subtitle" => root.get("subtitles").cloned(),
        other => root.get(other).cloned(),
    }
}

fn payload_is_empty(payload: &Value) -> bool {
    match payload {
        Value::Null => true,
        Value::Array(values) => values.is_empty(),
        Value::Object(values) => values.is_empty(),
        _ => false,
    }
}

fn cache_value(root: &Value, key: &str) -> Value {
    root.get(key)
        .and_then(Value::as_i64)
        .map(|value| json!(value))
        .unwrap_or(Value::Null)
}

pub(crate) enum ParsedAddonBody {
    Error(String),
    Success { payload: Value, root: Value },
}

pub(crate) fn parse_addon_body(
    resource: &str,
    url: &str,
    status_code: i32,
    body: Option<&str>,
) -> ParsedAddonBody {
    if !(200..=299).contains(&status_code) {
        return ParsedAddonBody::Error(
            json!({
                "kind": "network_error",
                "url": url,
                "statusCode": status_code
            })
            .to_string(),
        );
    }

    let Some(body) = body.map(str::trim).filter(|value| !value.is_empty()) else {
        return ParsedAddonBody::Error(
            json!({
                "kind": "empty",
                "url": url,
                "statusCode": status_code
            })
            .to_string(),
        );
    };

    let root: Value = match serde_json::from_str(body) {
        Ok(root) => root,
        Err(error) => {
            return ParsedAddonBody::Error(
                json!({
                    "kind": "parse_error",
                    "url": url,
                    "statusCode": status_code,
                    "error": error.to_string()
                })
                .to_string(),
            )
        }
    };

    let Some(payload) = resource_payload(resource, &root) else {
        return ParsedAddonBody::Error(
            json!({
                "kind": "empty",
                "url": url,
                "statusCode": status_code
            })
            .to_string(),
        );
    };

    if payload_is_empty(&payload) {
        return ParsedAddonBody::Error(
            json!({
                "kind": "empty",
                "url": url,
                "statusCode": status_code
            })
            .to_string(),
        );
    }

    ParsedAddonBody::Success { payload, root }
}

pub(crate) fn parse_addon_resource_result_json(
    resource: &str,
    url: &str,
    status_code: i32,
    body: Option<&str>,
) -> String {
    match parse_addon_body(resource, url, status_code, body) {
        ParsedAddonBody::Error(json_str) => json_str,
        ParsedAddonBody::Success { payload, root } => json!({
            "kind": "success",
            "url": url,
            "statusCode": status_code,
            "cacheMaxAge": cache_value(&root, "cacheMaxAge"),
            "staleRevalidate": cache_value(&root, "staleRevalidate"),
            "staleError": cache_value(&root, "staleError"),
            "valueJson": payload.to_string()
        })
        .to_string(),
    }
}

pub(crate) fn wrap_addon_resource_response_value(resource: &str, payload: Value) -> Value {
    match resource {
        "catalog" | "metas" => json!({ "metas": payload }),
        "stream" | "streams" => json!({ "streams": payload }),
        "meta" => json!({ "meta": payload }),
        "subtitle" | "subtitles" => json!({ "subtitles": payload }),
        _ => payload,
    }
}

pub(crate) fn normalize_addon_subtitles_json(subtitles_json: &str, resource_url: &str) -> String {
    let subtitles = serde_json::from_str::<Vec<Value>>(subtitles_json)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|mut subtitle| {
            let object = subtitle.as_object_mut()?;
            let explicit_url = object
                .get("url")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string);
            let attribute_url = object
                .get("attributes")
                .and_then(Value::as_object)
                .and_then(|attributes| attributes.get("url"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string);
            let resolved_url =
                resolve_resource_asset_url(explicit_url.or(attribute_url), resource_url)?;

            object.insert("url".to_string(), Value::String(resolved_url.clone()));
            let lang = object
                .get("lang")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string);
            let attributes = object
                .entry("attributes".to_string())
                .or_insert_with(|| json!({}));
            if !attributes.is_object() {
                *attributes = json!({});
            }
            let attributes = attributes.as_object_mut()?;
            let attribute_url_is_blank = attributes
                .get("url")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .is_empty();
            if attribute_url_is_blank {
                attributes.insert("url".to_string(), Value::String(resolved_url));
            }
            let languages_empty = attributes
                .get("languages")
                .and_then(Value::as_array)
                .map(Vec::is_empty)
                .unwrap_or(true);
            if languages_empty {
                if let Some(lang) = lang {
                    attributes.insert("languages".to_string(), json!([lang]));
                }
            }
            Some(subtitle)
        })
        .collect::<Vec<_>>();
    Value::Array(subtitles).to_string()
}

fn resolve_resource_asset_url(asset: Option<String>, resource_url: &str) -> Option<String> {
    let secure = addon_protocol::prefer_https_asset_url(asset?.as_str())?;
    if addon_protocol::is_http_url(&secure) {
        return Some(secure);
    }
    if secure.starts_with('/') {
        let scheme_end = resource_url.find("://").map(|index| index + 3)?;
        let host_end = resource_url[scheme_end..]
            .find('/')
            .map(|index| scheme_end + index)
            .unwrap_or(resource_url.len());
        return addon_protocol::prefer_https_asset_url(&format!(
            "{}{}",
            &resource_url[..host_end],
            secure
        ));
    }
    let base = resource_url
        .rsplit_once('/')
        .map(|(base, _)| format!("{base}/"))
        .unwrap_or_default();
    addon_protocol::prefer_https_asset_url(&format!("{base}{secure}"))
}

#[cfg(test)]
mod tests {
    use super::{normalize_addon_subtitles_json, parse_addon_resource_result_json};
    use serde_json::Value;

    #[test]
    fn stream_payload_keeps_provider_order_and_content() {
        let result = parse_addon_resource_result_json(
            "stream",
            "https://addon.example/stream/movie/tt1.json",
            200,
            Some(
                r#"{"streams":[{"title":"B"},{"title":"A"}],"cacheMaxAge":3600,"staleRevalidate":120,"staleError":60}"#,
            ),
        );
        let result: Value = serde_json::from_str(&result).expect("result json");
        let value_json = result
            .get("valueJson")
            .and_then(Value::as_str)
            .expect("value json");
        let streams: Value = serde_json::from_str(value_json).expect("streams");

        assert_eq!(result.get("kind").and_then(Value::as_str), Some("success"));
        assert_eq!(
            streams
                .get(0)
                .and_then(|item| item.get("title"))
                .and_then(Value::as_str),
            Some("B")
        );
        assert_eq!(
            streams
                .get(1)
                .and_then(|item| item.get("title"))
                .and_then(Value::as_str),
            Some("A")
        );
        assert_eq!(
            result.get("cacheMaxAge").and_then(Value::as_i64),
            Some(3600)
        );
        assert_eq!(
            result.get("staleRevalidate").and_then(Value::as_i64),
            Some(120)
        );
        assert_eq!(result.get("staleError").and_then(Value::as_i64), Some(60));
    }

    #[test]
    fn empty_and_error_states_are_classified_without_platform_code() {
        let empty =
            parse_addon_resource_result_json("catalog", "url", 200, Some(r#"{"metas":[]}"#));
        let network = parse_addon_resource_result_json("catalog", "url", 503, Some("{}"));
        let parse = parse_addon_resource_result_json("catalog", "url", 200, Some("{"));

        assert_eq!(
            serde_json::from_str::<Value>(&empty)
                .ok()
                .and_then(|value| value.get("kind").and_then(Value::as_str).map(str::to_owned))
                .as_deref(),
            Some("empty")
        );
        assert_eq!(
            serde_json::from_str::<Value>(&network)
                .ok()
                .and_then(|value| value.get("kind").and_then(Value::as_str).map(str::to_owned))
                .as_deref(),
            Some("network_error")
        );
        assert_eq!(
            serde_json::from_str::<Value>(&parse)
                .ok()
                .and_then(|value| value.get("kind").and_then(Value::as_str).map(str::to_owned))
                .as_deref(),
            Some("parse_error")
        );
    }

    #[test]
    fn subtitle_payload_is_resolved_without_reordering_valid_entries() {
        let subtitles = normalize_addon_subtitles_json(
            r#"[
                {"id":"one","url":"/subs/one.vtt","lang":"en","attributes":{"languages":[]}},
                {"id":"drop","attributes":{}},
                {"id":"two","attributes":{"url":"two.srt"}}
            ]"#,
            "https://addon.example/subtitles/movie/tt1.json",
        );
        let subtitles: Value = serde_json::from_str(&subtitles).expect("subtitles");

        assert_eq!(subtitles.as_array().map(Vec::len), Some(2));
        assert_eq!(subtitles[0]["id"].as_str(), Some("one"));
        assert_eq!(
            subtitles[0]["url"].as_str(),
            Some("https://addon.example/subs/one.vtt")
        );
        assert_eq!(
            subtitles[0]["attributes"]["languages"][0].as_str(),
            Some("en")
        );
        assert_eq!(subtitles[1]["id"].as_str(), Some("two"));
        assert_eq!(
            subtitles[1]["url"].as_str(),
            Some("https://addon.example/subtitles/movie/two.srt")
        );
    }
}
