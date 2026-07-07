use crate::local_stream::build_proxy_client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const INNERTUBE_URL: &str = "https://www.youtube.com/youtubei/v1/player";
const INNERTUBE_API_KEY: &str = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";
const CACHE_TTL_SECS: u64 = 6 * 60 * 60;
const CACHE_FILE_NAME: &str = "youtube_trailer_cache.json";

struct ClientContext {
    x_youtube_client_name: &'static str,
    client_version: &'static str,
    user_agent: &'static str,
    client_json: fn() -> serde_json::Value,
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
}

#[derive(Deserialize)]
struct PlayabilityStatus {
    status: Option<String>,
}

#[derive(Deserialize)]
struct StreamingData {
    formats: Option<Vec<Format>>,
    #[serde(rename = "hlsManifestUrl")]
    hls_manifest_url: Option<String>,
}

#[derive(Deserialize)]
struct Format {
    url: Option<String>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    bitrate: Option<i64>,
}

#[derive(serde::Serialize, Deserialize, Default)]
struct TrailerCache {
    entries: HashMap<String, CacheEntry>,
}

#[derive(serde::Serialize, Deserialize, Clone)]
struct CacheEntry {
    url: String,
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

pub fn resolve_youtube_trailer_stream_url(video_id: &str, cache_dir: &str) -> Option<String> {
    if video_id.is_empty() {
        return None;
    }
    let path = cache_path(cache_dir);
    let mut cache = load_cache(&path);
    if let Some(entry) = cache.entries.get(video_id) {
        if now_secs().saturating_sub(entry.fetched_at) < CACHE_TTL_SECS {
            return Some(entry.url.clone());
        }
    }

    let client = build_proxy_client();
    for ctx in CLIENT_CONTEXTS {
        if let Some(url) = fetch_player_stream_url(&client, video_id, ctx) {
            cache.entries.insert(
                video_id.to_string(),
                CacheEntry {
                    url: url.clone(),
                    fetched_at: now_secs(),
                },
            );
            save_cache(&path, &cache);
            return Some(url);
        }
    }
    None
}

fn fetch_player_stream_url(
    client: &reqwest::blocking::Client,
    video_id: &str,
    ctx: &ClientContext,
) -> Option<String> {
    let body = json!({
        "videoId": video_id,
        "context": { "client": (ctx.client_json)() },
    });

    let url = format!("{INNERTUBE_URL}?key={INNERTUBE_API_KEY}");
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", ctx.user_agent)
        .header("X-YouTube-Client-Name", ctx.x_youtube_client_name)
        .header("X-YouTube-Client-Version", ctx.client_version)
        .json(&body)
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let parsed: PlayerResponse = response.json().ok()?;
    if let Some(status) = parsed
        .playability_status
        .as_ref()
        .and_then(|p| p.status.as_deref())
    {
        if status != "OK" {
            return None;
        }
    }
    let streaming = parsed.streaming_data?;
    if let Some(best) = streaming
        .formats
        .unwrap_or_default()
        .into_iter()
        .filter(|f| {
            f.url.is_some()
                && f.mime_type.as_deref().map_or(false, |m: &str| m.starts_with("video/mp4"))
        })
        .max_by_key(|f| f.bitrate.unwrap_or(0))
    {
        return best.url;
    }
    streaming.hls_manifest_url
}
