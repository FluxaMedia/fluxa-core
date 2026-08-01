mod crypto_bridge;
mod dom_bridge;
mod host_functions;
mod scraper_exec;
mod settings_layout;

use scraper_exec::run;
use serde::{Deserialize, Serialize};
use settings_layout::run_settings_layout;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub(super) const PLUGIN_TIMEOUT_SECS: u64 = 60;
pub(super) const PLUGIN_MEMORY_LIMIT: usize = 256 * 1024 * 1024;
pub const PLUGIN_MAX_REQUEST_BODY_BYTES: usize = 1_048_576;
pub const PLUGIN_MAX_RESPONSE_BODY_BYTES: usize = 8 * 1_048_576;

pub fn plugin_http_request_error(request: &PluginHttpRequest) -> Option<&'static str> {
    if !matches!(
        request.method.to_ascii_uppercase().as_str(),
        "GET" | "HEAD" | "POST"
    ) {
        return Some("plugin request method is not allowed");
    }
    let lower = request.url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Some("only http and https plugin URLs are allowed");
    }
    let authority = lower
        .split("//")
        .nth(1)?
        .split('/')
        .next()?
        .split('@')
        .next_back()?;
    let host = authority.split(':').next().unwrap_or(authority);
    let private_172 = host
        .strip_prefix("172.")
        .and_then(|rest| rest.split('.').next())
        .and_then(|part| part.parse::<u8>().ok())
        .is_some_and(|second| (16..=31).contains(&second));
    if host == "localhost"
        || host.ends_with(".localhost")
        || host == "::1"
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.starts_with("0.")
        || private_172
        || host.starts_with("fc")
        || host.starts_with("fd")
        || host.starts_with("fe80:")
    {
        return Some("private or loopback plugin URL is not allowed");
    }
    if let Some(body) = &request.body
        && body.len() > PLUGIN_MAX_REQUEST_BODY_BYTES
    {
        return Some("plugin request body exceeds limit");
    }
    if request.headers.keys().any(|key| {
        !matches!(
            key.to_ascii_lowercase().as_str(),
            "accept" | "accept-language" | "content-type" | "origin" | "referer" | "user-agent"
        )
    }) {
        return Some("plugin request contains a disallowed header");
    }
    None
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi-bindings", derive(uniffi::Record))]
#[serde(rename_all = "camelCase")]
pub struct PluginHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub follow_redirects: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi-bindings", derive(uniffi::Record))]
#[serde(rename_all = "camelCase")]
pub struct PluginHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[cfg_attr(feature = "uniffi-bindings", uniffi::export(callback_interface))]
pub trait PluginHttpClient: Send + Sync {
    fn fetch(&self, request: PluginHttpRequest) -> PluginHttpResponse;
}

#[expect(
    clippy::too_many_arguments,
    reason = "public scraper bridge mirrors the established FFI wire contract"
)]
pub fn execute_scraper(
    client: Arc<dyn PluginHttpClient>,
    code: String,
    scraper_id: String,
    scraper_settings_json: String,
    tmdb_id: String,
    media_type: String,
    season: Option<i32>,
    episode: Option<i32>,
) -> Result<String, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        tokio::time::timeout(
            Duration::from_secs(PLUGIN_TIMEOUT_SECS),
            run(
                client,
                code,
                scraper_id,
                scraper_settings_json,
                tmdb_id,
                media_type,
                season,
                episode,
            ),
        )
        .await
        .unwrap_or_else(|_| Err("plugin timed out".to_string()))
    })
}

pub fn get_settings_layout(code: String, scraper_id: String) -> String {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return "[]".to_string(),
    };
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        tokio::time::timeout(
            Duration::from_secs(PLUGIN_TIMEOUT_SECS),
            run_settings_layout(code, scraper_id),
        )
        .await
        .unwrap_or_else(|_| "[]".to_string())
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    struct MockHttpClient;

    impl PluginHttpClient for MockHttpClient {
        fn fetch(&self, _request: PluginHttpRequest) -> PluginHttpResponse {
            PluginHttpResponse {
                status: 0,
                headers: HashMap::new(),
                body: String::new(),
                ok: false,
                error: Some("network disabled in unit tests".to_string()),
            }
        }
    }

    fn mock_client() -> Arc<dyn PluginHttpClient> {
        Arc::new(MockHttpClient)
    }

    #[test]
    fn plugin_request_policy_rejects_private_hosts_and_credentials() {
        let mut request = PluginHttpRequest {
            url: "http://127.0.0.1/admin".to_string(),
            ..PluginHttpRequest::default()
        };
        assert!(plugin_http_request_error(&request).is_some());
        request.url = "https://example.com/".to_string();
        request
            .headers
            .insert("Authorization".to_string(), "secret".to_string());
        assert!(plugin_http_request_error(&request).is_some());
    }

    fn run_scraper(code: &str, tmdb_id: &str, media_type: &str) -> String {
        execute_scraper(
            mock_client(),
            code.to_string(),
            "test-scraper".to_string(),
            "{}".to_string(),
            tmdb_id.to_string(),
            media_type.to_string(),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn scraper_can_use_cheerio_and_module_exports_without_network() {
        let code = r#"
            module.exports.getStreams = async function(tmdbId, mediaType) {
                var $ = cheerio.load('<div class="row" data-q="1080p">Alpha</div>');
                var el = $('.row');
                return [{ title: el.attr('data-q'), url: 'https://example.com/' + tmdbId + '-' + mediaType }];
            };
        "#;

        let result = run_scraper(code, "123", "movie");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[0]["title"], "1080p");
        assert_eq!(parsed[0]["url"], "https://example.com/123-movie");
    }

    #[test]
    fn scraper_that_throws_returns_empty_array_instead_of_erroring() {
        let code = r#"
            module.exports.getStreams = async function() {
                throw new Error("boom");
            };
        "#;

        let result = run_scraper(code, "1", "movie");
        assert_eq!(result, "[]");
    }

    #[test]
    fn fetch_bridge_surfaces_host_rejection_as_not_ok() {
        let code = r#"
            module.exports.getStreams = async function() {
                var res = await fetch('https://example.com/secret');
                return [{ title: 'x', url: 'https://example.com/blocked', ok: res.ok }];
            };
        "#;

        let result = run_scraper(code, "1", "movie");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[0]["ok"], false);
    }

    #[test]
    fn cryptojs_sha256_matches_known_vector() {
        let code = r#"
            module.exports.getStreams = async function() {
                var hash = CryptoJS.SHA256('abc').toString(CryptoJS.enc.Hex);
                return [{ title: hash, url: 'https://example.com/x' }];
            };
        "#;
        let result = run_scraper(code, "1", "movie");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed[0]["title"],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn cryptojs_hmac_sha256_matches_known_vector() {
        let code = r#"
            module.exports.getStreams = async function() {
                var mac = CryptoJS.HmacSHA256('The quick brown fox jumps over the lazy dog', 'key').toString(CryptoJS.enc.Hex);
                return [{ title: mac, url: 'https://example.com/x' }];
            };
        "#;
        let result = run_scraper(code, "1", "movie");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed[0]["title"],
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn cryptojs_aes_cbc_roundtrips_with_explicit_key_and_iv() {
        let code = r#"
            module.exports.getStreams = async function() {
                var key = CryptoJS.enc.Utf8.parse('0123456789abcdef');
                var iv = CryptoJS.enc.Utf8.parse('abcdef9876543210');
                var plaintext = 'secret stream url payload';
                var encrypted = CryptoJS.AES.encrypt(plaintext, key, { iv: iv });
                var decrypted = CryptoJS.AES.decrypt(encrypted, key, { iv: iv }).toString(CryptoJS.enc.Utf8);
                return [{ title: decrypted, url: 'https://example.com/x' }];
            };
        "#;
        let result = run_scraper(code, "1", "movie");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[0]["title"], "secret stream url payload");
    }

    #[test]
    fn webcrypto_subtle_digest_matches_cryptojs() {
        let code = r#"
            module.exports.getStreams = async function() {
                var buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode('abc'));
                var hex = Array.from(new Uint8Array(buf)).map(function(b) {
                    return b.toString(16).padStart(2, '0');
                }).join('');
                return [{ title: hex, url: 'https://example.com/x' }];
            };
        "#;
        let result = run_scraper(code, "1", "movie");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            parsed[0]["title"],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn webcrypto_ecdsa_verify_accepts_a_python_produced_signature() {
        let key_hex = "3059301306072a8648ce3d020106082a8648ce3d030107034200049e6a723242258b9d87c8362fd321140b80e16d1671b5cb9b2dba3da7ccc42b82380102e3fd415ed40dfc8c4b4b218218995327daedc7eb35493f4f5b419aeaf8";
        let sig_hex = "f982b6fd52964b591329f5b503627dc1dc5b7f74ff0cf9acc840ab160636a99a526a7f11ee179e77d176827ab0035ee92653e3e7408c6c8fea3f566ec79e8c8f";
        let code = format!(
            r#"
            module.exports.getStreams = async function() {{
                function hexToBytes(hex) {{
                    var bytes = new Uint8Array(hex.length / 2);
                    for (var i = 0; i < hex.length; i += 2) bytes[i / 2] = parseInt(hex.substr(i, 2), 16);
                    return bytes;
                }}
                var keyBytes = hexToBytes('{key_hex}');
                var sigBytes = hexToBytes('{sig_hex}');
                var key = await crypto.subtle.importKey('spki', keyBytes, {{ name: 'ECDSA', namedCurve: 'P-256' }}, false, ['verify']);
                var ok = await crypto.subtle.verify({{ name: 'ECDSA', hash: 'SHA-256' }}, key, sigBytes, new TextEncoder().encode('hello plugin signature test'));
                return [{{ title: String(ok), url: 'https://example.com/x' }}];
            }};
            "#,
            key_hex = key_hex,
            sig_hex = sig_hex,
        );
        let result = run_scraper(&code, "1", "movie");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[0]["title"], "true");
    }

    #[test]
    fn scraper_id_and_settings_are_visible_as_globals() {
        let code = r#"
            module.exports.getStreams = async function() {
                return [{ title: SCRAPER_ID + ':' + SCRAPER_SETTINGS.quality, url: 'https://example.com/x' }];
            };
        "#;
        let result = execute_scraper(
            mock_client(),
            code.to_string(),
            "my-scraper".to_string(),
            r#"{"quality":"1080p"}"#.to_string(),
            "1".to_string(),
            "movie".to_string(),
            None,
            None,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[0]["title"], "my-scraper:1080p");
    }

    #[test]
    fn settings_layout_returns_declared_field_layout() {
        let code = r#"
            module.exports.onSettings = async function() {
                return [
                    { type: 'header', label: 'General' },
                    { key: 'apiKey', type: 'text', label: 'API Key', isPassword: true },
                    {
                        key: 'quality',
                        type: 'select',
                        label: 'Preferred Quality',
                        defaultValue: '1080p',
                        options: [
                            { label: '720p', value: '720p' },
                            { label: '1080p', value: '1080p' }
                        ]
                    },
                    { key: 'enabled', type: 'toggle', label: 'Enable Feature', defaultValue: true }
                ];
            };
        "#;

        let result = get_settings_layout(code.to_string(), "my-scraper".to_string());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 4);
        assert_eq!(parsed[1]["key"], "apiKey");
        assert_eq!(parsed[2]["options"][1]["value"], "1080p");
        assert_eq!(parsed[3]["defaultValue"], true);
    }

    #[test]
    fn settings_layout_defaults_to_empty_array_when_undefined() {
        let code = r#"
            module.exports.getStreams = async function() { return []; };
        "#;

        let result = get_settings_layout(code.to_string(), "my-scraper".to_string());
        assert_eq!(result, "[]");
    }
}
