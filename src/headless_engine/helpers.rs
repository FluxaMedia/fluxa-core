use super::state::EngineState;
use crate::constants::GUEST_PROFILE_ID;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize)]
struct NormalizedTrailer {
    id: String,
    title: String,
    #[serde(rename = "type")]
    item_type: String,
    url: String,
    thumbnail: Option<String>,
    source: &'static str,
}

pub(super) fn normalize_error(error: Value) -> Value {
    if error.is_null() {
        json!({ "code": "generic" })
    } else {
        error
    }
}

pub(super) fn error_code(error: &Value) -> String {
    error["code"]
        .as_str()
        .or_else(|| error.as_str())
        .unwrap_or("generic")
        .to_string()
}

pub(super) fn active_profile_id(state: &EngineState, profile: &Value) -> String {
    profile["id"]
        .as_str()
        .or_else(|| state.profile.active_profile_id.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(GUEST_PROFILE_ID)
        .to_string()
}

pub(super) fn visible_streams(streams: &Value, selected_addon: Option<&str>) -> Value {
    let Some(selected_addon) = selected_addon
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return streams.clone();
    };
    let selected_lower = selected_addon.to_lowercase();
    let filtered = streams
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|stream| {
                    stream["addonName"]
                        .as_str()
                        .map(|addon_name| addon_name.trim().to_lowercase() == selected_lower)
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!(filtered)
}

pub(super) fn value_array_is_empty(value: &Value) -> bool {
    value.as_array().map(Vec::is_empty).unwrap_or(true)
}

pub(super) fn normalize_meta_trailers(meta: &Value) -> Value {
    let trailers = meta["trailers"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(normalize_meta_trailer)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(trailers)
}

fn normalize_meta_trailer(trailer: &Value) -> Option<Value> {
    let youtube_id = non_blank_str(trailer, "ytId").or_else(|| {
        non_blank_str(trailer, "source")
            .filter(|value| !value.starts_with("http://") && !value.starts_with("https://"))
    });
    let url = non_blank_str(trailer, "externalUrl")
        .or_else(|| non_blank_str(trailer, "url"))
        .or_else(|| {
            youtube_id
                .as_ref()
                .map(|value| format!("https://www.youtube.com/watch?v={value}"))
        })?;
    let id = youtube_id.clone().unwrap_or_else(|| url.clone());
    let item_type = non_blank_str(trailer, "type").unwrap_or_else(|| "Trailer".to_string());
    let title = non_blank_str(trailer, "name")
        .or_else(|| non_blank_str(trailer, "title"))
        .or_else(|| non_blank_str(trailer, "description"))
        .unwrap_or_else(|| item_type.clone());
    let thumbnail = non_blank_str(trailer, "thumbnail").or_else(|| {
        youtube_id
            .as_ref()
            .map(|value| format!("https://i.ytimg.com/vi/{value}/hqdefault.jpg"))
    });
    serde_json::to_value(NormalizedTrailer {
        id,
        title,
        item_type,
        url,
        thumbnail,
        source: "addon",
    })
    .ok()
}

pub(super) fn non_blank_str(value: &Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub(super) fn should_sync_watched_state(profile: Option<&Value>, meta: Option<&Value>) -> bool {
    let Some(meta) = meta else { return false };
    if meta["id"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .is_none()
    {
        return false;
    }
    let Some(profile) = profile else { return false };
    let is_guest = profile["isGuest"].as_bool().unwrap_or(false);
    let has_trakt_token = profile["traktAccessToken"]
        .as_str()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    !is_guest || has_trakt_token
}

pub(super) fn upsert_by_key(target: &mut Value, key: &str, value: &str, item: Value) {
    if !target.is_array() {
        *target = json!([]);
    }
    let Some(items) = target.as_array_mut() else {
        return;
    };
    if let Some(existing) = items
        .iter_mut()
        .find(|existing| existing[key].as_str() == Some(value))
    {
        *existing = item;
    } else {
        items.push(item);
    }
}
