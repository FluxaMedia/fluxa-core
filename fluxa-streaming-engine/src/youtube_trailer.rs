use crate::local_stream::build_proxy_client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const INNERTUBE_URL: &str = "https://www.youtube.com/youtubei/v1/player";
const INNERTUBE_API_KEY: &str = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";
const CACHE_TTL_SECS: u64 = 6 * 60 * 60;
const CACHE_FILE_NAME: &str = "youtube_trailer_cache.json";
const WATCH_CONFIG_FILE_NAME: &str = "youtube_watch_config.json";
const WATCH_CONFIG_TTL_SECS: u64 = 3 * 60 * 60;

struct ClientContext {
    x_youtube_client_name: &'static str,
    client_version: &'static str,
    user_agent: &'static str,
    client_json: fn() -> serde_json::Value,
}

fn android_vr_client_json() -> serde_json::Value {
    json!({
        "clientName": "ANDROID_VR",
        "clientVersion": "1.56.21",
        "deviceMake": "Oculus",
        "deviceModel": "Quest 3",
        "osName": "Android",
        "osVersion": "12",
        "platform": "MOBILE",
        "androidSdkVersion": 32,
        "hl": "en",
        "gl": "US",
    })
}

fn android_client_json() -> serde_json::Value {
    json!({
        "clientName": "ANDROID",
        "clientVersion": "21.02.35",
        "androidSdkVersion": 30,
        "userAgent": "com.google.android.youtube/21.02.35 (Linux; U; Android 11) gzip",
        "osName": "Android",
        "osVersion": "11",
        "hl": "en",
        "gl": "US",
    })
}

fn ios_client_json() -> serde_json::Value {
    json!({
        "clientName": "IOS",
        "clientVersion": "21.02.3",
        "deviceMake": "Apple",
        "deviceModel": "iPhone16,2",
        "userAgent": "com.google.ios.youtube/21.02.3 (iPhone16,2; U; CPU iOS 18_3_2 like Mac OS X;)",
        "osName": "iPhone",
        "osVersion": "18.3.2.22D82",
        "hl": "en",
        "gl": "US",
    })
}

const CLIENT_CONTEXTS: &[ClientContext] = &[
    ClientContext {
        x_youtube_client_name: "28",
        client_version: "1.56.21",
        user_agent: "com.google.android.apps.youtube.vr.oculus/1.56.21 (Linux; U; Android 12; en_US; Quest 3; Build/SQ3A.220605.009.A1) gzip",
        client_json: android_vr_client_json,
    },
    ClientContext {
        x_youtube_client_name: "3",
        client_version: "21.02.35",
        user_agent: "com.google.android.youtube/21.02.35 (Linux; U; Android 11) gzip",
        client_json: android_client_json,
    },
    ClientContext {
        x_youtube_client_name: "5",
        client_version: "21.02.3",
        user_agent: "com.google.ios.youtube/21.02.3 (iPhone16,2; U; CPU iOS 18_3_2 like Mac OS X;)",
        client_json: ios_client_json,
    },
];

#[derive(Deserialize)]
struct PlayerResponse {
    #[serde(rename = "playabilityStatus")]
    playability_status: Option<PlayabilityStatus>,
    #[serde(rename = "streamingData")]
    streaming_data: Option<StreamingData>,
    captions: Option<Captions>,
}

#[derive(Deserialize)]
struct PlayabilityStatus {
    status: Option<String>,
}

#[derive(Deserialize)]
struct StreamingData {
    formats: Option<Vec<Format>>,
    #[serde(rename = "adaptiveFormats")]
    adaptive_formats: Option<Vec<Format>>,
    #[serde(rename = "hlsManifestUrl")]
    hls_manifest_url: Option<String>,
}

#[derive(Deserialize)]
struct Format {
    url: Option<String>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    bitrate: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    #[serde(rename = "audioQuality")]
    audio_quality: Option<String>,
}

#[derive(Deserialize)]
struct Captions {
    #[serde(rename = "playerCaptionsTracklistRenderer")]
    tracklist_renderer: Option<CaptionsTracklistRenderer>,
}

#[derive(Deserialize)]
struct CaptionsTracklistRenderer {
    #[serde(rename = "captionTracks")]
    caption_tracks: Option<Vec<CaptionTrack>>,
}

#[derive(Deserialize)]
struct CaptionTrack {
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(rename = "languageCode")]
    language_code: Option<String>,
    kind: Option<String>,
    name: Option<CaptionName>,
}

#[derive(Deserialize)]
struct CaptionName {
    #[serde(rename = "simpleText")]
    simple_text: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SubtitleTrack {
    #[serde(rename = "languageTag")]
    language_tag: String,
    label: String,
    url: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "isAuto")]
    is_auto: bool,
}

struct TrailerStream {
    stream_url: String,
    audio_url: Option<String>,
    subtitles: Vec<SubtitleTrack>,
    kind: TrailerStreamKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrailerStreamKind {
    Hls,
    AdaptivePair,
    MuxedMp4,
}

enum PlayerOutcome {
    Ok(TrailerStream),
    GeoBlocked,
    Failed,
}

#[derive(Serialize, Deserialize, Default)]
struct TrailerCache {
    entries: HashMap<String, CacheEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
struct CacheEntry {
    url: String,
    #[serde(default, rename = "audioUrl")]
    audio_url: Option<String>,
    #[serde(default)]
    subtitles: Vec<SubtitleTrack>,
    fetched_at: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_path(cache_dir: &str) -> PathBuf {
    PathBuf::from(cache_dir).join(CACHE_FILE_NAME)
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct WatchConfig {
    api_key: String,
    visitor_data: Option<String>,
    fetched_at: u64,
}

fn watch_config_path(cache_dir: &str) -> PathBuf {
    PathBuf::from(cache_dir).join(WATCH_CONFIG_FILE_NAME)
}

fn load_watch_config(path: &PathBuf) -> Option<WatchConfig> {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
}

fn save_watch_config(path: &PathBuf, config: &WatchConfig) {
    if let Ok(json) = serde_json::to_string(config) {
        let _ = fs::write(path, json);
    }
}

fn extract_json_string_field(html: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = html.find(&needle)? + needle.len();
    let rest = &html[start..];
    let mut end = 0;
    let bytes = rest.as_bytes();
    while end < bytes.len() {
        if bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\') {
            break;
        }
        end += 1;
    }
    Some(rest[..end].to_string())
}

fn fetch_watch_config(
    client: &reqwest::blocking::Client,
    cache_dir: &str,
    force_refresh: bool,
) -> WatchConfig {
    let path = watch_config_path(cache_dir);
    if !force_refresh {
        if let Some(config) = load_watch_config(&path) {
            if now_secs().saturating_sub(config.fetched_at) < WATCH_CONFIG_TTL_SECS {
                return config;
            }
        }
    }

    let response = client
        .get("https://www.youtube.com/watch?v=dQw4w9WgXcQ&hl=en")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.text().ok());

    let Some(html) = response else {
        return load_watch_config(&path).unwrap_or_default();
    };

    let config = WatchConfig {
        api_key: extract_json_string_field(&html, "INNERTUBE_API_KEY")
            .unwrap_or_else(|| INNERTUBE_API_KEY.to_string()),
        visitor_data: extract_json_string_field(&html, "VISITOR_DATA"),
        fetched_at: now_secs(),
    };
    save_watch_config(&path, &config);
    config
}

fn load_cache(path: &PathBuf) -> TrailerCache {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_cache(path: &PathBuf, cache: &TrailerCache) {
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = fs::write(path, json);
    }
}

pub fn prewarm_youtube_watch_config(cache_dir: &str) {
    let client = build_proxy_client();
    fetch_watch_config(&client, cache_dir, false);
}

pub fn resolve_youtube_trailer_stream_url(video_id: &str, cache_dir: &str) -> Option<String> {
    let json = resolve_youtube_trailer_json(video_id, cache_dir)?;
    let parsed: serde_json::Value = serde_json::from_str(&json).ok()?;
    if parsed.get("status").and_then(|v| v.as_str()) != Some("ok") {
        return None;
    }
    parsed
        .get("streamUrl")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

pub fn resolve_youtube_trailer_json(video_id: &str, cache_dir: &str) -> Option<String> {
    if video_id.is_empty() {
        return None;
    }
    let path = cache_path(cache_dir);
    let mut cache = load_cache(&path);
    if let Some(entry) = cache.entries.get(video_id) {
        if now_secs().saturating_sub(entry.fetched_at) < CACHE_TTL_SECS {
            return Some(
                json!({
                    "status": "ok",
                    "streamUrl": entry.url,
                    "audioUrl": entry.audio_url,
                    "subtitles": entry.subtitles,
                })
                .to_string(),
            );
        }
    }

    let client = build_proxy_client();
    let watch_config = fetch_watch_config(&client, cache_dir, false);
    let mut outcome = try_all_clients(&client, video_id, &watch_config);
    if matches!(outcome, PlayerOutcome::GeoBlocked | PlayerOutcome::Failed) {
        let refreshed_config = fetch_watch_config(&client, cache_dir, true);
        outcome = try_all_clients(&client, video_id, &refreshed_config);
    }

    match outcome {
        PlayerOutcome::Ok(stream) => {
            cache.entries.insert(
                video_id.to_string(),
                CacheEntry {
                    url: stream.stream_url.clone(),
                    audio_url: stream.audio_url.clone(),
                    subtitles: stream.subtitles.clone(),
                    fetched_at: now_secs(),
                },
            );
            save_cache(&path, &cache);
            Some(
                json!({
                    "status": "ok",
                    "streamUrl": stream.stream_url,
                    "audioUrl": stream.audio_url,
                    "subtitles": stream.subtitles,
                })
                .to_string(),
            )
        }
        PlayerOutcome::GeoBlocked => Some(json!({ "status": "geo_blocked" }).to_string()),
        PlayerOutcome::Failed => Some(json!({ "status": "failed" }).to_string()),
    }
}

fn try_all_clients(
    client: &reqwest::blocking::Client,
    video_id: &str,
    watch_config: &WatchConfig,
) -> PlayerOutcome {
    let mut last_outcome = PlayerOutcome::Failed;
    let mut adaptive_fallback: Option<TrailerStream> = None;
    let mut muxed_fallback: Option<TrailerStream> = None;
    for ctx in CLIENT_CONTEXTS {
        match fetch_player_stream(client, video_id, ctx, watch_config) {
            PlayerOutcome::Ok(stream) if stream.kind == TrailerStreamKind::Hls => {
                return PlayerOutcome::Ok(stream);
            }
            PlayerOutcome::Ok(stream) if stream.kind == TrailerStreamKind::AdaptivePair => {
                if adaptive_fallback.is_none() {
                    adaptive_fallback = Some(stream);
                }
            }
            PlayerOutcome::Ok(stream) => {
                if muxed_fallback.is_none() {
                    muxed_fallback = Some(stream);
                }
            }
            outcome => last_outcome = outcome,
        }
    }
    adaptive_fallback
        .or(muxed_fallback)
        .map(PlayerOutcome::Ok)
        .unwrap_or(last_outcome)
}

fn fetch_player_stream(
    client: &reqwest::blocking::Client,
    video_id: &str,
    ctx: &ClientContext,
    watch_config: &WatchConfig,
) -> PlayerOutcome {
    let body = json!({
        "videoId": video_id,
        "contentCheckOk": true,
        "racyCheckOk": true,
        "context": { "client": (ctx.client_json)() },
    });

    let url = format!("{INNERTUBE_URL}?key={}", watch_config.api_key);
    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", ctx.user_agent)
        .header("X-YouTube-Client-Name", ctx.x_youtube_client_name)
        .header("X-YouTube-Client-Version", ctx.client_version);
    if let Some(visitor_data) = &watch_config.visitor_data {
        request = request.header("X-Goog-Visitor-Id", visitor_data);
    }
    let response = match request.json(&body).send() {
        Ok(response) => response,
        Err(_) => return PlayerOutcome::Failed,
    };
    if !response.status().is_success() {
        return PlayerOutcome::Failed;
    }
    let parsed: PlayerResponse = match response.json() {
        Ok(parsed) => parsed,
        Err(_) => return PlayerOutcome::Failed,
    };
    if let Some(status) = parsed
        .playability_status
        .as_ref()
        .and_then(|p| p.status.as_deref())
    {
        if status == "UNPLAYABLE" || status == "LOGIN_REQUIRED" {
            return PlayerOutcome::GeoBlocked;
        }
        if status != "OK" {
            return PlayerOutcome::Failed;
        }
    }
    let subtitles = extract_subtitles(parsed.captions.as_ref());
    let Some(streaming) = parsed.streaming_data else {
        return PlayerOutcome::Failed;
    };
    if let Some(url) = streaming.hls_manifest_url {
        let stream_url = best_hls_stream_url(client, &url).unwrap_or(url);
        return PlayerOutcome::Ok(TrailerStream {
            stream_url,
            audio_url: None,
            subtitles,
            kind: TrailerStreamKind::Hls,
        });
    }
    if let Some((stream_url, audio_url)) = best_adaptive_pair(streaming.adaptive_formats) {
        return PlayerOutcome::Ok(TrailerStream {
            stream_url,
            audio_url: Some(audio_url),
            subtitles,
            kind: TrailerStreamKind::AdaptivePair,
        });
    }
    if let Some(url) = best_muxed_mp4(streaming.formats) {
        return PlayerOutcome::Ok(TrailerStream {
            stream_url: url,
            audio_url: None,
            subtitles,
            kind: TrailerStreamKind::MuxedMp4,
        });
    }
    PlayerOutcome::Failed
}

fn best_hls_stream_url(client: &reqwest::blocking::Client, manifest_url: &str) -> Option<String> {
    let text = client
        .get(manifest_url)
        .header(
            "Accept",
            "application/vnd.apple.mpegurl, application/x-mpegURL, */*",
        )
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .ok()?;
    select_hls_variant_url(manifest_url, &text)
}

fn select_hls_variant_url(master_url: &str, manifest: &str) -> Option<String> {
    let mut pending_stream_inf: Option<&str> = None;
    let mut best: Option<(i64, i64, String)> = None;

    for line in manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending_stream_inf = Some(attrs);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let Some(attrs) = pending_stream_inf.take() else {
            continue;
        };
        if !hls_variant_has_muxed_audio(attrs) {
            continue;
        }
        let pixels = hls_resolution_pixels(attrs);
        let bandwidth = hls_attr_i64(attrs, "BANDWIDTH").unwrap_or(0);
        let url = absolutize_hls_url(master_url, line)?;
        let score = (pixels, bandwidth, url);
        if best
            .as_ref()
            .map_or(true, |current| (score.0, score.1) > (current.0, current.1))
        {
            best = Some(score);
        }
    }

    best.map(|(_, _, url)| url)
}

fn hls_variant_has_muxed_audio(attrs: &str) -> bool {
    !hls_has_attr(attrs, "AUDIO")
        && hls_attr_value(attrs, "CODECS").map_or(false, |codecs| codecs.contains("mp4a."))
}

fn hls_resolution_pixels(attrs: &str) -> i64 {
    let Some(resolution) = hls_attr_value(attrs, "RESOLUTION") else {
        return 0;
    };
    let Some((width, height)) = resolution.split_once('x') else {
        return 0;
    };
    let width = width.trim().parse::<i64>().unwrap_or(0);
    let height = height.trim().parse::<i64>().unwrap_or(0);
    width.saturating_mul(height)
}

fn hls_attr_i64(attrs: &str, key: &str) -> Option<i64> {
    hls_attr_value(attrs, key)?.parse::<i64>().ok()
}

fn hls_has_attr(attrs: &str, key: &str) -> bool {
    hls_attr_value(attrs, key).is_some()
}

fn hls_attr_value<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=");
    let start = attrs.find(&needle)? + needle.len();
    let rest = &attrs[start..];
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(&stripped[..end]);
    }
    let end = rest.find(',').unwrap_or(rest.len());
    Some(rest[..end].trim())
}

fn absolutize_hls_url(master_url: &str, uri: &str) -> Option<String> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return Some(uri.to_string());
    }
    reqwest::Url::parse(master_url)
        .ok()?
        .join(uri)
        .ok()
        .map(|url| url.to_string())
}

fn best_muxed_mp4(formats: Option<Vec<Format>>) -> Option<String> {
    formats?
        .into_iter()
        .filter(|f| f.url.is_some() && f.is_muxed_mp4())
        .max_by_key(|f| {
            (
                f.height.unwrap_or(0),
                f.width.unwrap_or(0),
                f.bitrate.unwrap_or(0),
            )
        })
        .and_then(|f| f.url)
}

impl Format {
    fn is_muxed_mp4(&self) -> bool {
        let Some(mime_type) = self.mime_type.as_deref() else {
            return false;
        };
        mime_type.starts_with("video/mp4")
            && (self.audio_quality.is_some() || mime_type.contains("mp4a."))
    }
}

fn best_adaptive_pair(formats: Option<Vec<Format>>) -> Option<(String, String)> {
    let formats = formats?;
    let video = formats
        .iter()
        .filter(|f| f.is_adaptive_video_mp4())
        .max_by_key(|f| {
            (
                f.height.unwrap_or(0),
                f.width.unwrap_or(0),
                f.video_codec_score(),
                f.bitrate.unwrap_or(0),
            )
        })?;
    let audio = formats
        .iter()
        .filter(|f| f.is_adaptive_audio_mp4())
        .max_by_key(|f| f.bitrate.unwrap_or(0))?;
    Some((video.url.clone()?, audio.url.clone()?))
}

impl Format {
    fn is_adaptive_video_mp4(&self) -> bool {
        let Some(mime_type) = self.mime_type.as_deref() else {
            return false;
        };
        self.url.is_some()
            && self.audio_quality.is_none()
            && self.height.unwrap_or(0) > 0
            && mime_type.starts_with("video/mp4")
    }

    fn is_adaptive_audio_mp4(&self) -> bool {
        let Some(mime_type) = self.mime_type.as_deref() else {
            return false;
        };
        self.url.is_some() && self.audio_quality.is_some() && mime_type.starts_with("audio/mp4")
    }

    fn video_codec_score(&self) -> i64 {
        let mime_type = self.mime_type.as_deref().unwrap_or_default();
        if mime_type.contains("avc1") {
            3
        } else if mime_type.contains("hvc1") || mime_type.contains("hev1") {
            2
        } else if mime_type.contains("av01") {
            1
        } else {
            0
        }
    }
}

fn extract_subtitles(captions: Option<&Captions>) -> Vec<SubtitleTrack> {
    let mut seen = std::collections::HashSet::new();
    captions
        .and_then(|c| c.tracklist_renderer.as_ref())
        .and_then(|r| r.caption_tracks.as_ref())
        .into_iter()
        .flatten()
        .filter_map(|track| {
            let base_url = track.base_url.as_deref()?;
            if !base_url.starts_with("http") {
                return None;
            }
            let language_tag = track.language_code.clone().filter(|s| !s.is_empty())?;
            if !seen.insert(language_tag.clone()) {
                return None;
            }
            let is_auto = track.kind.as_deref() == Some("asr");
            let name = track
                .name
                .as_ref()
                .and_then(|n| n.simple_text.clone())
                .unwrap_or_else(|| language_tag.clone());
            Some(SubtitleTrack {
                label: if is_auto {
                    format!("{name} (auto)")
                } else {
                    name
                },
                url: format!("{base_url}&fmt=vtt"),
                mime_type: "text/vtt".to_string(),
                is_auto,
                language_tag,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_hls_variant_prefers_highest_muxed_stream() {
        let manifest = r#"
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1200000,CODECS="avc1.4d401e,mp4a.40.2",RESOLUTION=854x480
low.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=5800000,CODECS="avc1.640028,mp4a.40.2",RESOLUTION=1920x1080
high.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=3000000,CODECS="avc1.4d401f,mp4a.40.2",RESOLUTION=1280x720
mid.m3u8
"#;

        assert_eq!(
            select_hls_variant_url("https://video.example/master.m3u8", manifest).as_deref(),
            Some("https://video.example/high.m3u8")
        );
    }

    #[test]
    fn select_hls_variant_skips_separate_audio_streams() {
        let manifest = r#"
#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="English",URI="audio.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=9000000,CODECS="avc1.640028",RESOLUTION=1920x1080,AUDIO="audio"
video-only.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=3000000,CODECS="avc1.4d401f,mp4a.40.2",RESOLUTION=1280x720
safe.m3u8
"#;

        assert_eq!(
            select_hls_variant_url("https://video.example/master.m3u8", manifest).as_deref(),
            Some("https://video.example/safe.m3u8")
        );
    }

    #[test]
    fn best_adaptive_pair_selects_highest_video_and_best_audio() {
        let formats = vec![
            Format {
                url: Some("https://video.example/360.mp4".to_string()),
                mime_type: Some("video/mp4; codecs=\"avc1.42001E\"".to_string()),
                bitrate: Some(500_000),
                width: Some(640),
                height: Some(360),
                audio_quality: None,
            },
            Format {
                url: Some("https://video.example/1080.mp4".to_string()),
                mime_type: Some("video/mp4; codecs=\"avc1.640028\"".to_string()),
                bitrate: Some(4_000_000),
                width: Some(1920),
                height: Some(1080),
                audio_quality: None,
            },
            Format {
                url: Some("https://video.example/audio-low.m4a".to_string()),
                mime_type: Some("audio/mp4; codecs=\"mp4a.40.2\"".to_string()),
                bitrate: Some(96_000),
                width: None,
                height: None,
                audio_quality: Some("AUDIO_QUALITY_LOW".to_string()),
            },
            Format {
                url: Some("https://video.example/audio-high.m4a".to_string()),
                mime_type: Some("audio/mp4; codecs=\"mp4a.40.2\"".to_string()),
                bitrate: Some(128_000),
                width: None,
                height: None,
                audio_quality: Some("AUDIO_QUALITY_MEDIUM".to_string()),
            },
        ];

        assert_eq!(
            best_adaptive_pair(Some(formats)),
            Some((
                "https://video.example/1080.mp4".to_string(),
                "https://video.example/audio-high.m4a".to_string()
            ))
        );
    }
}
