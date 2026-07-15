use super::state::GenerationKey;
use super::{EffectResultInput, HeadlessEngine};
use crate::runtime::{EffectEnvelope, EffectKind};
use serde::Serialize;
use serde_json::{json, Value};

const WATCH_URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&hl=en";
const PLAYER_URL: &str = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
const DEFAULT_API_KEY: &str = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct TrailerState {
    pub(super) resolutions: std::collections::HashMap<String, Value>,
    #[serde(skip)]
    requests: std::collections::HashMap<String, String>,
    #[serde(skip)]
    watch_config: Option<WatchConfig>,
}

#[derive(Clone, Debug)]
struct WatchConfig {
    api_key: String,
    visitor_data: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpEffectPayload {
    request_id: Option<String>,
    url: String,
    method: String,
    headers: Value,
    body: Option<Value>,
}

pub(super) fn dispatch_resolve(
    engine: &mut HeadlessEngine,
    request_id: String,
    video_id: String,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Trailer);
    engine.state.trailer.resolutions.remove(&request_id);
    engine
        .state
        .trailer
        .requests
        .insert(request_id.clone(), video_id);
    if engine.state.trailer.watch_config.is_some() {
        return dispatch_player(engine, generation, &request_id);
    }
    vec![watch_config_effect(engine, generation, Some(request_id))]
}

pub(super) fn dispatch_prewarm(engine: &mut HeadlessEngine) -> Vec<EffectEnvelope> {
    if engine.state.trailer.watch_config.is_some() {
        return vec![];
    }
    let generation = engine.bump_generation(GenerationKey::Trailer);
    vec![watch_config_effect(engine, generation, None)]
}

pub(super) fn complete(
    engine: &mut HeadlessEngine,
    effect_type: &str,
    generation: u64,
    effect: &EffectEnvelope,
    result: &EffectResultInput,
) -> Vec<EffectEnvelope> {
    if generation != engine.state.runtime.get(GenerationKey::Trailer) {
        return vec![];
    }
    let request_id = effect
        .payload
        .get("requestId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match effect_type {
        "fetchYoutubeTrailerWatchConfig" => {
            if result.status.is_ok() {
                engine.state.trailer.watch_config = Some(parse_watch_config(&result.value));
            }
            request_id
                .as_deref()
                .map(|id| dispatch_player(engine, generation, id))
                .unwrap_or_default()
        }
        "fetchYoutubeTrailerPlayer" => {
            let Some(request_id) = request_id else {
                return vec![];
            };
            let resolution = if result.status.is_ok() {
                resolve_player_response(&result.value)
            } else {
                Value::Null
            };
            engine.state.trailer.requests.remove(&request_id);
            engine
                .state
                .trailer
                .resolutions
                .insert(request_id, resolution);
            vec![]
        }
        _ => vec![],
    }
}

fn watch_config_effect(
    engine: &mut HeadlessEngine,
    generation: u64,
    request_id: Option<String>,
) -> EffectEnvelope {
    engine.effect(
        EffectKind::FetchYoutubeTrailerWatchConfig,
        generation,
        HttpEffectPayload {
            request_id,
            url: WATCH_URL.to_string(),
            method: "GET".to_string(),
            headers: json!({ "Accept-Language": "en-US,en;q=0.9" }),
            body: None,
        },
    )
}

fn dispatch_player(
    engine: &mut HeadlessEngine,
    generation: u64,
    request_id: &str,
) -> Vec<EffectEnvelope> {
    let Some(video_id) = engine.state.trailer.requests.get(request_id).cloned() else {
        return vec![];
    };
    let config = engine
        .state
        .trailer
        .watch_config
        .clone()
        .unwrap_or(WatchConfig {
            api_key: DEFAULT_API_KEY.to_string(),
            visitor_data: None,
        });
    let mut headers = json!({
        "User-Agent": "com.google.android.apps.youtube.vr.oculus/1.56.21 (Linux; U; Android 12; en_US; Quest 3; Build/SQ3A.220605.009.A1) gzip",
        "X-YouTube-Client-Name": "28",
        "X-YouTube-Client-Version": "1.56.21"
    });
    if let Some(visitor_data) = config.visitor_data {
        headers["X-Goog-Visitor-Id"] = Value::String(visitor_data);
    }
    vec![engine.effect(
        EffectKind::FetchYoutubeTrailerPlayer,
        generation,
        HttpEffectPayload {
            request_id: Some(request_id.to_string()),
            url: format!("{PLAYER_URL}&key={}", config.api_key),
            method: "POST".to_string(),
            headers,
            body: Some(json!({
                "videoId": video_id,
                "contentCheckOk": true,
                "racyCheckOk": true,
                "context": {
                    "client": {
                        "clientName": "ANDROID_VR",
                        "clientVersion": "1.56.21",
                        "deviceMake": "Oculus",
                        "deviceModel": "Quest 3",
                        "osName": "Android",
                        "osVersion": "12",
                        "platform": "MOBILE",
                        "androidSdkVersion": 32,
                        "hl": "en",
                        "gl": "US"
                    }
                },
                "playbackContext": { "contentPlaybackContext": { "html5Preference": "HTML5_PREF_WANTS" } }
            })),
        },
    )]
}

fn parse_watch_config(response: &Value) -> WatchConfig {
    let html = response
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    WatchConfig {
        api_key: extract_json_string_field(html, "INNERTUBE_API_KEY")
            .unwrap_or_else(|| DEFAULT_API_KEY.to_string()),
        visitor_data: extract_json_string_field(html, "VISITOR_DATA"),
    }
}

fn extract_json_string_field(html: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let rest = html.get(html.find(&needle)? + needle.len()..)?;
    let end = rest
        .as_bytes()
        .iter()
        .enumerate()
        .find_map(|(index, byte)| {
            (*byte == b'"' && (index == 0 || rest.as_bytes()[index - 1] != b'\\')).then_some(index)
        })?;
    Some(rest[..end].to_string())
}

fn resolve_player_response(response: &Value) -> Value {
    let payload = response
        .get("body")
        .and_then(Value::as_str)
        .and_then(|body| serde_json::from_str::<Value>(body).ok());
    let Some(payload) = payload else {
        return Value::Null;
    };
    if payload
        .pointer("/playabilityStatus/status")
        .and_then(Value::as_str)
        != Some("OK")
    {
        return Value::Null;
    }
    let adaptive_pair = best_adaptive_pair(payload.pointer("/streamingData/adaptiveFormats"));
    let progressive_url = first_direct_url(payload.pointer("/streamingData/formats"));
    let stream_url = adaptive_pair
        .as_ref()
        .map(|(video_url, _)| video_url.to_owned())
        .or_else(|| progressive_url)
        .or_else(|| {
            payload
                .pointer("/streamingData/hlsManifestUrl")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let Some(stream_url) = stream_url else {
        return Value::Null;
    };
    let audio_url = adaptive_pair
        .filter(|(video_url, _)| *video_url == stream_url)
        .map(|(_, audio_url)| audio_url);
    let subtitles = payload
        .pointer("/captions/playerCaptionsTracklistRenderer/captionTracks")
        .and_then(Value::as_array)
        .map(|tracks| tracks.iter().filter_map(caption_track).collect::<Vec<_>>())
        .unwrap_or_default();
    json!({ "status": "ok", "streamUrl": stream_url, "audioUrl": audio_url, "subtitles": subtitles })
}

fn first_direct_url(formats: Option<&Value>) -> Option<String> {
    formats?
        .as_array()?
        .iter()
        .find_map(|format| format.get("url").and_then(Value::as_str).map(str::to_owned))
}

fn best_adaptive_pair(formats: Option<&Value>) -> Option<(String, String)> {
    let entries = formats?.as_array()?;
    let video = entries
        .iter()
        .filter(|format| format.get("url").and_then(Value::as_str).is_some())
        .filter(|format| {
            format
                .get("mimeType")
                .and_then(Value::as_str)
                .is_some_and(|mime_type| mime_type.starts_with("video/mp4; codecs=\"avc1"))
        })
        .max_by_key(|format| {
            (
                format.get("height").and_then(Value::as_i64).unwrap_or(0),
                format.get("bitrate").and_then(Value::as_i64).unwrap_or(0),
            )
        })?
        .get("url")?
        .as_str()?
        .to_owned();
    let audio = entries
        .iter()
        .filter(|format| format.get("url").and_then(Value::as_str).is_some())
        .filter(|format| {
            format
                .get("mimeType")
                .and_then(Value::as_str)
                .is_some_and(|mime_type| mime_type.starts_with("audio/mp4"))
        })
        .max_by_key(|format| format.get("bitrate").and_then(Value::as_i64).unwrap_or(0))?
        .get("url")?
        .as_str()?
        .to_owned();
    Some((video, audio))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_highest_resolution_avc1_video_with_highest_bitrate_audio() {
        let formats = json!([
            { "url": "video-360p", "mimeType": "video/mp4; codecs=\"avc1.4d401e\"", "height": 360, "bitrate": 500_000 },
            { "url": "video-1080p", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
            { "url": "video-1080p-vp9", "mimeType": "video/webm; codecs=\"vp9\"", "height": 1080, "bitrate": 4_000_000 },
            { "url": "audio-low", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 64_000 },
            { "url": "audio-high", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 },
        ]);
        let pair = best_adaptive_pair(Some(&formats));
        assert_eq!(
            pair,
            Some(("video-1080p".to_string(), "audio-high".to_string()))
        );
    }

    #[test]
    fn no_pair_when_only_vp9_video_is_available() {
        let formats = json!([
            { "url": "video-vp9", "mimeType": "video/webm; codecs=\"vp9\"", "height": 1080, "bitrate": 3_000_000 },
            { "url": "audio", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 },
        ]);
        assert_eq!(best_adaptive_pair(Some(&formats)), None);
    }

    #[test]
    fn no_pair_when_audio_track_is_missing() {
        let formats = json!([
            { "url": "video", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
        ]);
        assert_eq!(best_adaptive_pair(Some(&formats)), None);
    }

    #[test]
    fn no_pair_when_adaptive_formats_are_absent() {
        assert_eq!(best_adaptive_pair(None), None);
    }

    #[test]
    fn first_direct_url_picks_first_entry_with_a_url() {
        let formats = json!([
            { "itag": 18 },
            { "url": "progressive-url", "itag": 22 },
        ]);
        assert_eq!(
            first_direct_url(Some(&formats)),
            Some("progressive-url".to_string())
        );
    }

    #[test]
    fn resolve_player_response_includes_audio_url_for_paired_adaptive_streams() {
        let body = json!({
            "playabilityStatus": { "status": "OK" },
            "streamingData": {
                "adaptiveFormats": [
                    { "url": "video-1080p", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
                    { "url": "audio", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 },
                ]
            }
        }).to_string();
        let response = json!({ "body": body });
        let resolved = resolve_player_response(&response);
        assert_eq!(resolved["streamUrl"], "video-1080p");
        assert_eq!(resolved["audioUrl"], "audio");
    }

    #[test]
    fn resolve_player_response_falls_back_to_progressive_when_no_adaptive_pair() {
        let body = json!({
            "playabilityStatus": { "status": "OK" },
            "streamingData": {
                "formats": [{ "url": "progressive-360p", "itag": 18 }]
            }
        })
        .to_string();
        let response = json!({ "body": body });
        let resolved = resolve_player_response(&response);
        assert_eq!(resolved["streamUrl"], "progressive-360p");
        assert!(resolved["audioUrl"].is_null());
    }
}

fn caption_track(track: &Value) -> Option<Value> {
    Some(json!({
        "languageTag": track.get("languageCode").and_then(Value::as_str).unwrap_or("und"),
        "label": track.pointer("/name/simpleText").and_then(Value::as_str).or_else(|| track.get("languageCode").and_then(Value::as_str)).unwrap_or(""),
        "url": track.get("baseUrl")?.as_str()?,
        "mimeType": "text/vtt",
        "isAuto": track.get("kind").and_then(Value::as_str) == Some("asr")
    }))
}
