use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::OnceLock;

#[derive(Deserialize)]
struct SearchPolicyRequest {
    query: String,
    #[serde(rename = "nowMillis")]
    now_millis: i64,
    #[serde(rename = "cachedAtMillis")]
    cached_at_millis: Option<i64>,
    #[serde(rename = "ttlMillis")]
    ttl_millis: i64,
}

pub(crate) fn addon_store_input_type(raw: &str) -> &'static str {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown";
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("manifest.json") {
        return "stremio_manifest";
    }
    if lower.starts_with("cloudstreamrepo://")
        || lower.starts_with("cloudstream://")
        || lower.contains("cloudstream")
        || lower.contains(".cs3")
        || lower.contains("repo.json")
        || ((lower.starts_with("http://") || lower.starts_with("https://")) && trimmed.len() > 20)
    {
        return "cloudstream_repo";
    }

    "search_query"
}

pub(crate) fn normalize_cloudstream_repo_url(raw: &str) -> String {
    let trimmed = raw.trim();
    replace_ascii_prefix(
        &replace_ascii_prefix(trimmed, "cloudstreamrepo://", "https://"),
        "cloudstream://",
        "https://",
    )
}

pub(crate) fn normalize_plugin_repository_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let Some(scheme_end) = trimmed.find("://") else {
        return trimmed.to_string();
    };
    let scheme = trimmed[..scheme_end].to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return format!("https://{}", &trimmed[scheme_end + 3..]);
    }
    replace_ascii_prefix(trimmed, "http://", "https://")
}

pub(crate) fn is_secure_remote_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("https://") {
        return false;
    }
    let host = trimmed["https://".len()..]
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim();
    !host.is_empty() && !host.contains(char::is_whitespace)
}

pub(crate) fn same_plugin_repository_url(left: &str, right: &str) -> bool {
    canonical_url_for_compare(left) == canonical_url_for_compare(right)
}

fn canonical_url_for_compare(raw: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("https://") {
        format!("https://{}", rest.trim_end_matches('/'))
    } else if let Some(rest) = lower.strip_prefix("http://") {
        format!("https://{}", rest.trim_end_matches('/'))
    } else {
        lower.trim_end_matches('/').to_string()
    }
}

pub(crate) fn profile_local_addons_key_json(profile_json: &str) -> Option<String> {
    let profile: Value = serde_json::from_str(profile_json).ok()?;
    let id = string_field(&profile, "id").unwrap_or_default();
    let email = string_field(&profile, "email").unwrap_or_default();
    Some(format!(
        "local_addons_{}",
        if id.trim().is_empty() { email } else { id }
    ))
}

pub(crate) fn sanitize_profile_json(
    profile_json: &str,
    mirrored_addons_json: &str,
    merge_mirrored_addons: bool,
) -> Option<String> {
    let mut profile: Value = serde_json::from_str(profile_json).ok()?;
    let mirrored_addons: Vec<String> = if merge_mirrored_addons {
        serde_json::from_str(mirrored_addons_json).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut base_addons = string_list_field(&profile, "localAddons");
    base_addons.extend(mirrored_addons);
    let cleaned_addons = normalize_distinct_addons(base_addons);
    let cleaned_ids = cleaned_addons
        .iter()
        .map(|addon| crate::addon_protocol::identity(addon))
        .collect::<std::collections::HashSet<_>>();
    let cleaned_disabled_addons =
        normalize_distinct_addons(string_list_field(&profile, "disabledLocalAddons"))
            .into_iter()
            .filter(|addon| cleaned_ids.contains(&crate::addon_protocol::identity(addon)))
            .collect::<Vec<_>>();

    let object = profile.as_object_mut()?;
    object.insert("localAddons".to_string(), json!(cleaned_addons));
    object.insert(
        "disabledLocalAddons".to_string(),
        json!(cleaned_disabled_addons),
    );
    fill_structured_settings(object);
    serde_json::to_string(&profile).ok()
}

pub(crate) fn addon_profile_mutation_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let mut profile = args.get("profile")?.clone();
    let command = args.get("command")?.as_str()?;
    let addon_key = args.get("addonKey")?.as_str()?;
    let settings = profile.get("addonSettings");
    let mut local = settings
        .and_then(|value| value.get("localAddons"))
        .and_then(Value::as_array)
        .or_else(|| profile.get("localAddons").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default();
    let mut disabled = settings
        .and_then(|value| value.get("disabledLocalAddons"))
        .and_then(Value::as_array)
        .or_else(|| profile.get("disabledLocalAddons").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default();
    let target = crate::addon_protocol::identity(addon_key);
    match command {
        "install" => {
            if !local.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|url| crate::addon_protocol::identity(url) == target)
            }) {
                local.push(Value::String(addon_key.to_string()));
            }
        }
        "remove" => {
            local.retain(|value| {
                value
                    .as_str()
                    .is_none_or(|url| crate::addon_protocol::identity(url) != target)
            });
            disabled.retain(|value| {
                value
                    .as_str()
                    .is_none_or(|url| crate::addon_protocol::identity(url) != target)
            });
        }
        "toggle" => {
            if disabled.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|url| crate::addon_protocol::identity(url) == target)
            }) {
                disabled.retain(|value| {
                    value
                        .as_str()
                        .is_none_or(|url| crate::addon_protocol::identity(url) != target)
                });
            } else {
                disabled.push(Value::String(addon_key.to_string()));
            }
        }
        _ => return None,
    }
    let object = profile.as_object_mut()?;
    object.insert("localAddons".to_string(), Value::Array(local.clone()));
    let settings = object
        .entry("addonSettings")
        .or_insert_with(|| json!({}))
        .as_object_mut()?;
    settings.insert("localAddons".to_string(), Value::Array(local));
    settings.insert("disabledLocalAddons".to_string(), Value::Array(disabled));
    serde_json::to_string(&profile).ok()
}

fn normalize_distinct_addons(addons: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    addons
        .into_iter()
        .map(|addon| crate::addon_protocol::normalize_manifest_url(&addon))
        .filter(|addon| !addon.trim().is_empty())
        .filter(|addon| seen.insert(crate::addon_protocol::identity(addon)))
        .collect()
}

fn fill_structured_settings(profile: &mut Map<String, Value>) {
    insert_object_from_fields(
        profile,
        "externalAccounts",
        &[
            ("traktAccessToken", "traktAccessToken"),
            ("traktRefreshToken", "traktRefreshToken"),
            ("traktTokenExpiresAt", "traktTokenExpiresAt"),
            ("traktLastSyncAt", "traktLastSyncAt"),
            ("traktLastSyncedItems", "traktLastSyncedItems"),
            (
                "traktLastContinueWatchingCount",
                "traktLastContinueWatchingCount",
            ),
            ("traktLastWatchlistCount", "traktLastWatchlistCount"),
            ("malAccessToken", "malAccessToken"),
            ("malRefreshToken", "malRefreshToken"),
            ("simklAccessToken", "simklAccessToken"),
        ],
    );
    insert_object_from_fields(
        profile,
        "addonSettings",
        &[
            ("localAddons", "localAddons"),
            ("disabledLocalAddons", "disabledLocalAddons"),
        ],
    );
    insert_object_from_fields(
        profile,
        "subtitleSettings",
        &[
            ("size", "subtitleSize"),
            ("color", "subtitleColor"),
            ("backgroundColor", "subtitleBackgroundColor"),
            ("outlineColor", "subtitleOutlineColor"),
            ("textOpacity", "subtitleTextOpacity"),
            ("backgroundOpacity", "subtitleBackgroundOpacity"),
            ("outlineOpacity", "subtitleOutlineOpacity"),
            ("preferredLanguage", "preferredSubtitleLanguage"),
            ("secondaryLanguage", "secondarySubtitleLanguage"),
            ("shadow", "subtitleShadow"),
            ("autoEnable", "autoEnableSubtitles"),
        ],
    );
    insert_object_from_fields(
        profile,
        "playbackSettings",
        &[
            ("preferredAudioLanguage", "preferredAudioLanguage"),
            ("secondaryAudioLanguage", "secondaryAudioLanguage"),
            ("stableVolume", "stableVolume"),
            ("ambientLight", "ambientLight"),
            ("forceSoftwareAudio", "forceSoftwareAudio"),
            ("preferredPlayer", "preferredPlayer"),
            ("autoSkipIntro", "autoSkipIntro"),
            ("autoPlayNextEpisode", "autoPlayNextEpisode"),
            ("nextEpisodeThresholdPercent", "nextEpisodeThresholdPercent"),
            ("watchedThresholdPercent", "watchedThresholdPercent"),
            ("seekForwardSeconds", "seekForwardSeconds"),
            ("seekBackwardSeconds", "seekBackwardSeconds"),
            ("playerBufferCacheMb", "playerBufferCacheMb"),
            ("playerForwardBufferSeconds", "playerForwardBufferSeconds"),
            ("playerBackBufferSeconds", "playerBackBufferSeconds"),
            ("backgroundPlayback", "backgroundPlayback"),
            ("pictureInPicture", "pictureInPicture"),
            ("playbackSpeed", "playbackSpeed"),
            ("holdToSpeedEnabled", "holdToSpeedEnabled"),
            ("holdSpeed", "holdSpeed"),
            ("dolbyVisionFallbackMode", "dolbyVisionFallbackMode"),
            ("dv7Fallback", "dv7Fallback"),
            ("dv7ToDv8Fallback", "dv7ToDv8Fallback"),
            ("tunneledPlayback", "tunneledPlayback"),
            ("useIntroDb", "useIntroDb"),
            ("useAniSkip", "useAniSkip"),
            ("defaultQuality", "defaultQuality"),
            ("mobileDataUsage", "mobileDataUsage"),
            ("hdrPlayback", "hdrPlayback"),
            ("resumePlayback", "resumePlayback"),
            ("autoplayMode", "autoplayMode"),
            ("streamSourceSelectionMode", "streamSourceSelectionMode"),
            ("streamSourceRegexPattern", "streamSourceRegexPattern"),
            ("tryBingeGroup", "tryBingeGroup"),
        ],
    );
    insert_object_from_fields(
        profile,
        "torrentSettings",
        &[
            ("wifiOnly", "torrentWifiOnly"),
            ("maxConnections", "torrentMaxConnections"),
            ("speedPreset", "torrentSpeedPreset"),
            ("cachePreset", "torrentCachePreset"),
        ],
    );
    insert_object_from_fields(
        profile,
        "appearanceSettings",
        &[
            ("language", "language"),
            ("cardLayout", "cardLayout"),
            ("continueWatchingLayout", "continueWatchingLayout"),
            ("continueWatchingArtwork", "continueWatchingArtwork"),
            ("continueWatchingEnabled", "continueWatchingEnabled"),
            ("appTheme", "appTheme"),
            ("accentColorArgb", "accentColorArgb"),
            ("cardCornerPreset", "cardCornerPreset"),
            ("interfaceDensity", "interfaceDensity"),
            ("amoledMode", "amoledMode"),
            ("posterWidthPreset", "posterWidthPreset"),
            ("posterLandscapeMode", "posterLandscapeMode"),
            ("posterHideTitles", "posterHideTitles"),
            ("detailEpisodeViewMode", "detailEpisodeViewMode"),
            ("detailSeasonSelectorMode", "detailSeasonSelectorMode"),
            ("detailSeasonPostersOnHero", "detailSeasonPostersOnHero"),
            ("homeSeasonPostersOnHero", "homeSeasonPostersOnHero"),
            ("animationsEnabled", "animationsEnabled"),
            ("reduceMotion", "reduceMotion"),
            ("startPage", "startPage"),
            ("continueWatchingSource", "continueWatchingSource"),
        ],
    );
    insert_object_from_fields(
        profile,
        "homeFeedSettings",
        &[
            ("heroFeedToggles", "heroFeedToggles"),
            ("homeFeedToggles", "homeFeedToggles"),
            ("topTenFeedToggles", "topTenFeedToggles"),
            ("heroFeedOrder", "heroFeedOrder"),
            ("homeFeedOrder", "homeFeedOrder"),
            ("showHeroSection", "showHeroSection"),
            ("libraryCollections", "libraryCollections"),
        ],
    );
}

fn insert_object_from_fields(
    profile: &mut Map<String, Value>,
    target: &str,
    fields: &[(&str, &str)],
) {
    let updates = fields
        .iter()
        .map(|(target_key, source_key)| {
            (
                (*target_key).to_string(),
                profile.get(*source_key).cloned().unwrap_or(Value::Null),
            )
        })
        .collect::<Vec<_>>();
    if profile.get(target).is_some_and(|value| !value.is_null()) {
        if let Some(target_object) = profile.get_mut(target).and_then(Value::as_object_mut) {
            for (key, value) in updates {
                target_object.insert(key, value);
            }
        }
        return;
    }
    let object = updates.into_iter().collect::<Map<_, _>>();
    profile.insert(target.to_string(), Value::Object(object));
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn string_list_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

pub(crate) fn addon_store_search_policy_json(request_json: &str) -> Option<String> {
    let request: SearchPolicyRequest = serde_json::from_str(request_json).ok()?;
    let normalized_query = request.query.trim().to_ascii_lowercase();
    if normalized_query.len() < 2 {
        return serde_json::to_string(&json!({
            "normalizedQuery": normalized_query,
            "url": "",
            "useCache": false,
            "shouldFetch": false
        }))
        .ok();
    }
    let use_cache = request
        .cached_at_millis
        .and_then(|cached_at| request.now_millis.checked_sub(cached_at))
        .map(|elapsed| elapsed <= request.ttl_millis)
        .unwrap_or(false);
    serde_json::to_string(&json!({
        "normalizedQuery": normalized_query,
        "url": format!(
            "https://stremio-addons.net/addons?query={}",
            form_urlencode(&normalized_query)
        ),
        "useCache": use_cache,
        "shouldFetch": !use_cache
    }))
    .ok()
}

fn addon_key(addon: &Value) -> String {
    addon
        .get("transportUrl")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            addon
                .get("manifest")
                .and_then(|manifest| manifest.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
}

pub(crate) fn filter_enabled_addons_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let addons = args.get("addons")?.as_array()?.clone();
    let disabled_keys: Vec<&str> = args
        .get("disabledKeys")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let filtered: Vec<Value> = addons
        .into_iter()
        .filter(|addon| !disabled_keys.contains(&addon_key(addon).as_str()))
        .collect();
    serde_json::to_string(&filtered).ok()
}

fn manifest_url_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"https?://[^"'\\ ]+manifest\.json[^"'\\ ]*"#)
            .expect("valid manifest url regex")
    })
}

pub(crate) fn extract_addon_manifest_url(detail_text: &str) -> Option<String> {
    let unescaped_text = detail_text.replace("\\/", "/").replace("\\u0026", "&");
    manifest_url_regex()
        .find(&unescaped_text)
        .map(|match_| match_.as_str().trim_end_matches('\\').to_string())
}

fn replace_ascii_prefix(value: &str, prefix: &str, replacement: &str) -> String {
    if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        format!("{replacement}{}", &value[prefix.len()..])
    } else {
        value.to_string()
    }
}

fn form_urlencode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'*');
        if keep {
            encoded.push(byte as char);
        } else if byte == b' ' {
            encoded.push('+');
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_manifest_before_generic_https_repo_rule() {
        assert_eq!(
            "stremio_manifest",
            addon_store_input_type("https://addon.example/manifest.json")
        );
        assert_eq!(
            "cloudstream_repo",
            addon_store_input_type("cloudstreamrepo://example.com/repo.json")
        );
        assert_eq!("search_query", addon_store_input_type("cinemeta"));
    }

    #[test]
    fn plans_search_url_and_cache_use() {
        let json = addon_store_search_policy_json(
            r#"{"query":"Game of Thrones","nowMillis":2000,"cachedAtMillis":1500,"ttlMillis":1000}"#,
        )
        .unwrap();
        assert!(json.contains(r#""normalizedQuery":"game of thrones""#));
        assert!(json.contains(r#""url":"https://stremio-addons.net/addons?query=game+of+thrones""#));
        assert!(json.contains(r#""useCache":true"#));
    }

    #[test]
    fn extracts_escaped_manifest_url_from_detail_page() {
        assert_eq!(
            Some("https://addon.example/root/manifest.json?x=1&y=2".to_string()),
            extract_addon_manifest_url(
                r#"<script>"https://addon.example\/root\/manifest.json?x=1\u0026y=2"</script>"#,
            )
        );
    }

    #[test]
    fn plugin_repository_url_policy_normalizes_and_requires_https() {
        assert_eq!(
            normalize_plugin_repository_url("cloudstream://example.com/repo.json"),
            "https://example.com/repo.json"
        );
        assert_eq!(
            normalize_plugin_repository_url("http://example.com/repo.json"),
            "https://example.com/repo.json"
        );
        assert!(is_secure_remote_url("https://example.com/repo.json"));
        assert!(!is_secure_remote_url("http://example.com/repo.json"));
        assert!(same_plugin_repository_url(
            "http://example.com/repo.json/",
            "https://EXAMPLE.com/repo.json"
        ));
    }

    #[test]
    fn sanitize_profile_merges_and_deduplicates_local_addons() {
        let sanitized = sanitize_profile_json(
            r#"{"id":"p1","email":"u@example.com","localAddons":["http://a.example/manifest.json"],"disabledLocalAddons":["https://b.example/manifest.json","https://missing.example/manifest.json"],"language":"tr"}"#,
            r#"["https://a.example/manifest.json","https://b.example/manifest.json"]"#,
            true,
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("profile");

        assert_eq!(
            sanitized
                .get("localAddons")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            sanitized
                .get("disabledLocalAddons")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            sanitized
                .get("appearanceSettings")
                .and_then(|value| value.get("language"))
                .and_then(Value::as_str),
            Some("tr")
        );
    }

    #[test]
    fn sanitize_profile_syncs_home_feed_settings_from_top_level_fields() {
        let sanitized = sanitize_profile_json(
            r#"{"id":"p1","email":"u@example.com","localAddons":["https://a.example/manifest.json"],"libraryCollections":[{"id":"new","title":"New"}],"homeFeedSettings":{"libraryCollections":[{"id":"old","title":"Old"}],"homeFeedToggles":["old"]},"homeFeedToggles":[]}"#,
            r#"[]"#,
            false,
        )
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .expect("profile");

        assert_eq!(
            sanitized["homeFeedSettings"]["libraryCollections"][0]["id"],
            "new"
        );
        assert_eq!(
            sanitized["homeFeedSettings"]["homeFeedToggles"]
                .as_array()
                .map(Vec::len),
            Some(0)
        );
    }
}
