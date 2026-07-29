use super::state::GenerationKey;
use super::youtube_cipher;
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
    requests: std::collections::HashMap<String, TrailerRequest>,
    #[serde(skip)]
    watch_config: Option<WatchConfig>,
}

#[derive(Clone, Debug)]
struct WatchConfig {
    api_key: String,
    visitor_data: Option<String>,
    player_script_url: Option<String>,
}

#[derive(Clone, Debug)]
struct TrailerRequest {
    video_id: String,
    max_height: Option<u32>,
    player_response: Option<Value>,
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
    max_height: Option<u32>,
) -> Vec<EffectEnvelope> {
    let generation = engine.bump_generation(GenerationKey::Trailer);
    engine.state.trailer.resolutions.remove(&request_id);
    engine.state.trailer.requests.insert(
        request_id.clone(),
        TrailerRequest {
            video_id,
            max_height,
            player_response: None,
        },
    );
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
            if result.status.is_ok() && requires_player_script(&result.value) {
                if let Some(script_url) = player_script_url(&result.value).or_else(|| {
                    engine
                        .state
                        .trailer
                        .watch_config
                        .as_ref()
                        .and_then(|config| config.player_script_url.clone())
                }) {
                    if let Some(request) = engine.state.trailer.requests.get_mut(&request_id) {
                        request.player_response = Some(result.value.clone());
                    }
                    return player_script_effect(engine, generation, &request_id, script_url);
                }
            }
            let resolution = if result.status.is_ok() {
                let max_height = engine
                    .state
                    .trailer
                    .requests
                    .get(&request_id)
                    .and_then(|request| request.max_height);
                resolve_player_response(&result.value, max_height, None)
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
        "fetchYoutubeTrailerPlayerScript" => {
            let Some(request_id) = request_id else {
                return vec![];
            };
            let request = engine.state.trailer.requests.remove(&request_id);
            let resolution = request
                .and_then(|request| {
                    let player_response = request.player_response?;
                    let player_js = result.value.get("body")?.as_str()?;
                    Some(resolve_player_response(
                        &player_response,
                        request.max_height,
                        Some(player_js),
                    ))
                })
                .unwrap_or(Value::Null);
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

fn player_script_effect(
    engine: &mut HeadlessEngine,
    generation: u64,
    request_id: &str,
    url: String,
) -> Vec<EffectEnvelope> {
    vec![engine.effect(
        EffectKind::FetchYoutubeTrailerPlayerScript,
        generation,
        HttpEffectPayload {
            request_id: Some(request_id.to_string()),
            url,
            method: "GET".to_string(),
            headers: json!({}),
            body: None,
        },
    )]
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
    let Some(request) = engine.state.trailer.requests.get(request_id).cloned() else {
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
            player_script_url: None,
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
                "videoId": request.video_id,
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
        player_script_url: extract_json_string_field(html, "PLAYER_JS_URL")
            .or_else(|| extract_json_string_field(html, "jsUrl"))
            .map(|url| normalize_youtube_url(&url)),
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

fn resolve_player_response(
    response: &Value,
    max_height: Option<u32>,
    player_js: Option<&str>,
) -> Value {
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
    let adaptive_pair = best_adaptive_pair(
        payload.pointer("/streamingData/adaptiveFormats"),
        max_height,
        player_js,
    );
    let progressive_url = first_direct_url(payload.pointer("/streamingData/formats"), player_js);
    let stream_url = adaptive_pair
        .as_ref()
        .map(|(video_url, _, _)| video_url.to_owned())
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
        .as_ref()
        .filter(|(video_url, _, _)| *video_url == stream_url)
        .map(|(_, audio_url, _)| audio_url.clone());
    let height = adaptive_pair
        .as_ref()
        .filter(|(video_url, _, _)| *video_url == stream_url)
        .map(|(_, _, height)| *height);
    let subtitles = payload
        .pointer("/captions/playerCaptionsTracklistRenderer/captionTracks")
        .and_then(Value::as_array)
        .map(|tracks| tracks.iter().filter_map(caption_track).collect::<Vec<_>>())
        .unwrap_or_default();
    json!({ "status": "ok", "streamUrl": stream_url, "audioUrl": audio_url, "height": height, "subtitles": subtitles })
}

fn player_script_url(response: &Value) -> Option<String> {
    response
        .get("body")?
        .as_str()
        .and_then(|body| serde_json::from_str::<Value>(body).ok())?
        .pointer("/assets/js")?
        .as_str()
        .map(normalize_youtube_url)
}

fn normalize_youtube_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://www.youtube.com{url}")
    }
}

fn requires_player_script(response: &Value) -> bool {
    response
        .get("body")
        .and_then(Value::as_str)
        .and_then(|body| serde_json::from_str::<Value>(body).ok())
        .is_some_and(|payload| {
            ["/streamingData/adaptiveFormats", "/streamingData/formats"]
                .iter()
                .filter_map(|pointer| payload.pointer(pointer).and_then(Value::as_array))
                .flatten()
                .any(format_requires_player_script)
        })
}

fn format_requires_player_script(format: &Value) -> bool {
    format.get("signatureCipher").is_some()
        || format.get("cipher").is_some()
        || format
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|url| {
                url.split_once('?').is_some_and(|(_, query)| {
                    query.split('#').next().is_some_and(|query| {
                        query
                            .split('&')
                            .any(|entry| entry.split_once('=').is_some_and(|(key, _)| key == "n"))
                    })
                })
            })
}

fn first_direct_url(formats: Option<&Value>, player_js: Option<&str>) -> Option<String> {
    formats?
        .as_array()?
        .iter()
        .find_map(|format| format_url(format, player_js))
}

fn best_adaptive_pair(
    formats: Option<&Value>,
    max_height: Option<u32>,
    player_js: Option<&str>,
) -> Option<(String, String, u32)> {
    let entries = formats?.as_array()?;
    let video = entries
        .iter()
        .filter(|format| format_url(format, player_js).is_some())
        .filter(|format| {
            format
                .get("mimeType")
                .and_then(Value::as_str)
                .is_some_and(|mime_type| mime_type.starts_with("video/mp4; codecs=\"avc1"))
        })
        .filter(|format| {
            max_height.is_none_or(|limit| {
                format
                    .get("height")
                    .and_then(Value::as_u64)
                    .is_some_and(|height| height <= limit as u64)
            })
        })
        .max_by_key(|format| {
            (
                format.get("height").and_then(Value::as_i64).unwrap_or(0),
                format.get("bitrate").and_then(Value::as_i64).unwrap_or(0),
            )
        })?;
    let height = video.get("height")?.as_u64()? as u32;
    let video = format_url(video, player_js)?;
    let audio_format = entries
        .iter()
        .filter(|format| format_url(format, player_js).is_some())
        .filter(|format| {
            format
                .get("mimeType")
                .and_then(Value::as_str)
                .is_some_and(|mime_type| mime_type.starts_with("audio/mp4"))
        })
        .max_by_key(|format| format.get("bitrate").and_then(Value::as_i64).unwrap_or(0))?;
    let audio = format_url(audio_format, player_js)?;
    Some((video, audio, height))
}

fn format_url(format: &Value, player_js: Option<&str>) -> Option<String> {
    format
        .get("url")
        .and_then(Value::as_str)
        .and_then(|url| youtube_cipher::resolve_url(url, player_js))
        .or_else(|| {
            player_js.and_then(|script| {
                format
                    .get("signatureCipher")
                    .or_else(|| format.get("cipher"))
                    .and_then(Value::as_str)
                    .and_then(|cipher| youtube_cipher::decipher_url(cipher, script))
            })
        })
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
        let pair = best_adaptive_pair(Some(&formats), None, None);
        assert_eq!(
            pair,
            Some(("video-1080p".to_string(), "audio-high".to_string(), 1080))
        );
    }

    #[test]
    fn mobile_pair_uses_720p_video_and_highest_audio_track() {
        let formats = json!([
            { "url": "video-360p", "mimeType": "video/mp4; codecs=\"avc1.4d401e\"", "height": 360, "bitrate": 500_000 },
            { "url": "video-720p", "mimeType": "video/mp4; codecs=\"avc1.4d401f\"", "height": 720, "bitrate": 1_800_000 },
            { "url": "video-1080p", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
            { "url": "audio-low", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 64_000 },
            { "url": "audio-high", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 }
        ]);
        assert_eq!(
            best_adaptive_pair(Some(&formats), Some(720), None),
            Some(("video-720p".to_string(), "audio-high".to_string(), 720))
        );
    }

    #[test]
    fn large_screen_pair_caps_video_at_1080p_and_keeps_highest_audio_track() {
        let formats = json!([
            { "url": "video-720p", "mimeType": "video/mp4; codecs=\"avc1.4d401f\"", "height": 720, "bitrate": 1_800_000 },
            { "url": "video-1080p", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
            { "url": "video-2160p", "mimeType": "video/mp4; codecs=\"avc1.640033\"", "height": 2160, "bitrate": 12_000_000 },
            { "url": "audio-low", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 64_000 },
            { "url": "audio-high", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 }
        ]);
        assert_eq!(
            best_adaptive_pair(Some(&formats), Some(1080), None),
            Some(("video-1080p".to_string(), "audio-high".to_string(), 1080))
        );
    }

    #[test]
    fn player_response_separates_mobile_and_large_screen_quality_caps() {
        let body = json!({
            "playabilityStatus": { "status": "OK" },
            "streamingData": {
                "adaptiveFormats": [
                    { "url": "video-720p", "mimeType": "video/mp4; codecs=\"avc1.4d401f\"", "height": 720, "bitrate": 1_800_000 },
                    { "url": "video-1080p", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
                    { "url": "video-2160p", "mimeType": "video/mp4; codecs=\"avc1.640033\"", "height": 2160, "bitrate": 12_000_000 },
                    { "url": "audio-low", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 64_000 },
                    { "url": "audio-high", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 }
                ]
            }
        })
        .to_string();
        let response = json!({ "body": body });
        let mobile = resolve_player_response(&response, Some(720), None);
        let large_screen = resolve_player_response(&response, Some(1080), None);

        assert_eq!(mobile["streamUrl"], "video-720p");
        assert_eq!(mobile["audioUrl"], "audio-high");
        assert_eq!(mobile["height"], 720);
        assert_eq!(large_screen["streamUrl"], "video-1080p");
        assert_eq!(large_screen["audioUrl"], "audio-high");
        assert_eq!(large_screen["height"], 1080);
    }

    #[test]
    fn supplied_youtube_videos_select_720p_on_mobile_and_1080p_on_large_screens() {
        for (video_id, formats) in [
            (
                "qgQunxD0qCk",
                json!([
                    { "itag": 136, "url": "qg-video-720", "mimeType": "video/mp4; codecs=\"avc1.4d401f\"", "height": 720, "bitrate": 1_800_000 },
                    { "itag": 137, "url": "qg-video-1080", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
                    { "itag": 140, "url": "qg-audio", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 }
                ]),
            ),
            (
                "WNhH00OIPP0",
                json!([
                    { "itag": 136, "url": "wnh-video-720", "mimeType": "video/mp4; codecs=\"avc1.4d401f\"", "height": 720, "bitrate": 1_800_000 },
                    { "itag": 137, "url": "wnh-video-1080", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
                    { "itag": 140, "url": "wnh-audio", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 }
                ]),
            ),
        ] {
            assert_eq!(
                best_adaptive_pair(Some(&formats), Some(720), None).map(|(_, _, height)| height),
                Some(720),
                "{video_id} mobile selection"
            );
            assert_eq!(
                best_adaptive_pair(Some(&formats), Some(1080), None).map(|(_, _, height)| height),
                Some(1080),
                "{video_id} large-screen selection"
            );
        }
    }

    #[test]
    fn ciphered_tracks_resolve_before_mobile_quality_selection() {
        let body = json!({
            "playabilityStatus": { "status": "OK" },
            "streamingData": {
                "adaptiveFormats": [
                    { "signatureCipher": "url=https%3A%2F%2Fvideo.example%2F720&s=abcdef&sp=sig", "mimeType": "video/mp4; codecs=\"avc1.4d401f\"", "height": 720, "bitrate": 1_800_000 },
                    { "signatureCipher": "url=https%3A%2F%2Fvideo.example%2F1080&s=abcdef&sp=sig", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
                    { "signatureCipher": "url=https%3A%2F%2Faudio.example%2Fhigh&s=abcdef&sp=sig", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 }
                ]
            }
        })
        .to_string();
        let response = json!({ "body": body });
        let player_js = r#"var XY={rv:function(a){a.reverse()}};sig=function(a){a=a.split("");XY.rv(a);return a.join("")}"#;
        let mobile = resolve_player_response(&response, Some(720), Some(player_js));
        assert_eq!(mobile["streamUrl"], "https://video.example/720?sig=fedcba");
        assert_eq!(mobile["audioUrl"], "https://audio.example/high?sig=fedcba");
        assert_eq!(mobile["height"], 720);
    }

    #[test]
    fn cipher_alias_and_n_parameter_are_resolved_before_large_screen_selection() {
        let body = json!({
            "playabilityStatus": { "status": "OK" },
            "streamingData": {
                "adaptiveFormats": [
                    { "cipher": "url=https%3A%2F%2Fvideo.example%2F720%3Fn%3Dabcdef&s=abcdef&sp=sig", "mimeType": "video/mp4; codecs=\"avc1.4d401f\"", "height": 720, "bitrate": 1_800_000 },
                    { "cipher": "url=https%3A%2F%2Fvideo.example%2F1080%3Fn%3Dabcdef&s=abcdef&sp=sig", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
                    { "url": "https://audio.example/high?n=abcdef", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 }
                ]
            }
        })
        .to_string();
        let response = json!({ "body": body });
        let player_js = r#"var XY={rv:function(a){a.reverse()}};sig=function(a){a=a.split("");XY.rv(a);return a.join("")};nfunc=function(a){a=a.split("");XY.rv(a);return a.join("")};var routes={n:nfunc}"#;
        let large_screen = resolve_player_response(&response, Some(1080), Some(player_js));
        assert_eq!(
            large_screen["streamUrl"],
            "https://video.example/1080?n=fedcba&sig=fedcba"
        );
        assert_eq!(
            large_screen["audioUrl"],
            "https://audio.example/high?n=fedcba"
        );
        assert_eq!(large_screen["height"], 1080);
    }

    #[test]
    fn player_script_is_requested_for_cipher_or_n_parameter_formats() {
        let response = json!({
            "body": json!({
                "streamingData": {
                    "adaptiveFormats": [
                        { "cipher": "url=https%3A%2F%2Fvideo.example%2F720&s=abcdef", "mimeType": "video/mp4" },
                        { "url": "https://audio.example/high?n=abcdef", "mimeType": "audio/mp4" }
                    ]
                }
            }).to_string()
        });
        assert!(requires_player_script(&response));
    }

    #[test]
    fn watch_config_uses_the_player_script_url_when_player_response_omits_assets() {
        let config = parse_watch_config(&json!({
            "body": r#"<script>ytcfg.set({"INNERTUBE_API_KEY":"key","VISITOR_DATA":"visitor","jsUrl":"/s/player/current/base.js"});</script>"#
        }));
        assert_eq!(config.api_key, "key");
        assert_eq!(config.visitor_data.as_deref(), Some("visitor"));
        assert_eq!(
            config.player_script_url.as_deref(),
            Some("https://www.youtube.com/s/player/current/base.js")
        );
    }

    #[test]
    fn player_response_assets_script_url_is_normalized_for_platform_http() {
        let response = json!({
            "body": json!({ "assets": { "js": "/s/player/current/base.js" } }).to_string()
        });
        assert_eq!(
            player_script_url(&response).as_deref(),
            Some("https://www.youtube.com/s/player/current/base.js")
        );
    }

    #[test]
    fn no_pair_when_only_vp9_video_is_available() {
        let formats = json!([
            { "url": "video-vp9", "mimeType": "video/webm; codecs=\"vp9\"", "height": 1080, "bitrate": 3_000_000 },
            { "url": "audio", "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 128_000 },
        ]);
        assert_eq!(best_adaptive_pair(Some(&formats), None, None), None);
    }

    #[test]
    fn no_pair_when_audio_track_is_missing() {
        let formats = json!([
            { "url": "video", "mimeType": "video/mp4; codecs=\"avc1.640028\"", "height": 1080, "bitrate": 3_000_000 },
        ]);
        assert_eq!(best_adaptive_pair(Some(&formats), None, None), None);
    }

    #[test]
    fn no_pair_when_adaptive_formats_are_absent() {
        assert_eq!(best_adaptive_pair(None, None, None), None);
    }

    #[test]
    fn first_direct_url_picks_first_entry_with_a_url() {
        let formats = json!([
            { "itag": 18 },
            { "url": "progressive-url", "itag": 22 },
        ]);
        assert_eq!(
            first_direct_url(Some(&formats), None),
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
        let resolved = resolve_player_response(&response, None, None);
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
        let resolved = resolve_player_response(&response, None, None);
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
