mod crypto_bridge;
mod dom_bridge;

use dom_bridge::DomBridge;
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PLUGIN_TIMEOUT_SECS: u64 = 60;
const PLUGIN_MEMORY_LIMIT: usize = 256 * 1024 * 1024;

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

async fn run_settings_layout(code: String, scraper_id: String) -> String {
    let qjs_rt = match AsyncRuntime::new() {
        Ok(rt) => rt,
        Err(_) => return "[]".to_string(),
    };
    qjs_rt.set_memory_limit(PLUGIN_MEMORY_LIMIT).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(PLUGIN_TIMEOUT_SECS);
    qjs_rt
        .set_interrupt_handler(Some(Box::new(move || std::time::Instant::now() > deadline)))
        .await;
    tokio::task::spawn_local(qjs_rt.drive());
    let ctx = match AsyncContext::full(&qjs_rt).await {
        Ok(ctx) => ctx,
        Err(_) => return "[]".to_string(),
    };

    let captured: Arc<Mutex<Option<String>>> = Default::default();
    let captured_clone = captured.clone();
    let dom = DomBridge::new();
    let client: Arc<dyn PluginHttpClient> = Arc::new(NoopHttpClient);

    let eval_result: rquickjs::Result<()> = ctx
        .async_with(async |ctx| {
            register_host_functions(&ctx, &dom, client)?;

            let scraper_id_arg = serde_json::to_string(&scraper_id).unwrap_or_else(|_| "\"\"".into());

            let script = format!(
                r#"
                globalThis.global = globalThis;
                globalThis.window = globalThis;
                globalThis.SCRAPER_ID = {scraper_id_arg};
                globalThis.SCRAPER_SETTINGS = {{}};

                function fetch(url, options) {{
                    options = options || {{}};
                    var method = options.method || 'GET';
                    var headersJson = JSON.stringify(options.headers || {{}});
                    var body = options.body === undefined || options.body === null ? null : String(options.body);
                    var followRedirects = options.redirect !== 'manual';
                    var raw = __native_fetch(url, method, headersJson, body, followRedirects);
                    var parsed = JSON.parse(raw);
                    return Promise.resolve({{
                        ok: parsed.ok,
                        status: parsed.status,
                        text: function() {{ return Promise.resolve(parsed.body); }},
                        json: function() {{
                            try {{ return Promise.resolve(JSON.parse(parsed.body)); }}
                            catch (e) {{ return Promise.resolve(null); }}
                        }}
                    }});
                }}

                {base64_polyfill}
                {text_encoder_polyfill}
                {crypto_polyfill}
                {cheerio_polyfill}

                var require = function(name) {{
                    if (name.indexOf('cheerio') !== -1) return cheerio;
                    if (name === 'crypto-js') return CryptoJS;
                    throw new Error('module not available: ' + name);
                }};

                var module = {{ exports: {{}} }};
                var exports = module.exports;
                (function() {{
                    {code}
                }})();

                (async function() {{
                    try {{
                        var onSettings = module.exports.onSettings || globalThis.onSettings;
                        if (!onSettings) {{
                            __capture_result(JSON.stringify([]));
                            return;
                        }}
                        var layout = await onSettings();
                        __capture_result(JSON.stringify(layout || []));
                    }} catch (e) {{
                        __capture_result(JSON.stringify([]));
                    }}
                }})();
                "#,
                base64_polyfill = BASE64_POLYFILL,
                text_encoder_polyfill = TEXT_ENCODER_POLYFILL,
                crypto_polyfill = CRYPTO_POLYFILL,
                cheerio_polyfill = CHEERIO_POLYFILL,
                code = code,
            );

            ctx.globals().set(
                "__capture_result",
                Function::new(ctx.clone(), move |s: String| {
                    *captured_clone.lock().expect("capture lock poisoned") = Some(s);
                })?,
            )?;

            ctx.eval::<(), _>(script).catch(&ctx).map_err(|e| {
                rquickjs::Error::new_from_js_message("plugin", "js", e.to_string())
            })?;

            Ok(())
        })
        .await;

    if eval_result.is_err() {
        return "[]".to_string();
    }
    qjs_rt.idle().await;

    let result = captured
        .lock()
        .expect("capture lock poisoned")
        .take()
        .unwrap_or_else(|| "[]".to_string());
    result
}

struct NoopHttpClient;

impl PluginHttpClient for NoopHttpClient {
    fn fetch(&self, _request: PluginHttpRequest) -> PluginHttpResponse {
        PluginHttpResponse {
            status: 0,
            headers: HashMap::new(),
            body: String::new(),
            ok: false,
            error: Some("network disabled in settings-layout evaluation".to_string()),
        }
    }
}

async fn run(
    client: Arc<dyn PluginHttpClient>,
    code: String,
    scraper_id: String,
    scraper_settings_json: String,
    tmdb_id: String,
    media_type: String,
    season: Option<i32>,
    episode: Option<i32>,
) -> Result<String, String> {
    let qjs_rt = AsyncRuntime::new().map_err(|e| e.to_string())?;
    qjs_rt.set_memory_limit(PLUGIN_MEMORY_LIMIT).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(PLUGIN_TIMEOUT_SECS);
    qjs_rt
        .set_interrupt_handler(Some(Box::new(move || std::time::Instant::now() > deadline)))
        .await;
    tokio::task::spawn_local(qjs_rt.drive());
    let ctx = AsyncContext::full(&qjs_rt)
        .await
        .map_err(|e| e.to_string())?;

    let captured: Arc<Mutex<Option<String>>> = Default::default();
    let captured_clone = captured.clone();
    let dom = DomBridge::new();

    let eval_result: rquickjs::Result<()> = ctx
        .async_with(async |ctx| {
            register_host_functions(&ctx, &dom, client)?;

            let scraper_id_arg = serde_json::to_string(&scraper_id).unwrap_or_else(|_| "\"\"".into());
            let scraper_settings_arg = if scraper_settings_json.trim().is_empty() {
                "{}".to_string()
            } else {
                scraper_settings_json.clone()
            };
            let tmdb_id_arg = serde_json::to_string(&tmdb_id).unwrap_or_else(|_| "\"\"".into());
            let media_type_arg =
                serde_json::to_string(&media_type).unwrap_or_else(|_| "\"movie\"".into());
            let season_arg = season.map(|s| s.to_string()).unwrap_or_else(|| "undefined".into());
            let episode_arg = episode.map(|e| e.to_string()).unwrap_or_else(|| "undefined".into());

            let script = format!(
                r#"
                globalThis.global = globalThis;
                globalThis.window = globalThis;
                globalThis.SCRAPER_ID = {scraper_id_arg};
                globalThis.SCRAPER_SETTINGS = {scraper_settings_arg};

                function fetch(url, options) {{
                    options = options || {{}};
                    var method = options.method || 'GET';
                    var headersJson = JSON.stringify(options.headers || {{}});
                    var body = options.body === undefined || options.body === null ? null : String(options.body);
                    var followRedirects = options.redirect !== 'manual';
                    var raw = __native_fetch(url, method, headersJson, body, followRedirects);
                    var parsed = JSON.parse(raw);
                    return Promise.resolve({{
                        ok: parsed.ok,
                        status: parsed.status,
                        text: function() {{ return Promise.resolve(parsed.body); }},
                        json: function() {{
                            try {{ return Promise.resolve(JSON.parse(parsed.body)); }}
                            catch (e) {{ return Promise.resolve(null); }}
                        }}
                    }});
                }}

                {base64_polyfill}
                {text_encoder_polyfill}
                {crypto_polyfill}
                {cheerio_polyfill}

                var require = function(name) {{
                    if (name.indexOf('cheerio') !== -1) return cheerio;
                    if (name === 'crypto-js') return CryptoJS;
                    throw new Error('module not available: ' + name);
                }};

                var module = {{ exports: {{}} }};
                var exports = module.exports;
                (function() {{
                    {code}
                }})();

                (async function() {{
                    try {{
                        var getStreams = module.exports.getStreams || globalThis.getStreams;
                        if (!getStreams) {{
                            __capture_result(JSON.stringify([]));
                            return;
                        }}
                        var streams = await getStreams({tmdb_id_arg}, {media_type_arg}, {season_arg}, {episode_arg});
                        __capture_result(JSON.stringify(streams || []));
                    }} catch (e) {{
                        __capture_result(JSON.stringify([]));
                    }}
                }})();
                "#,
                base64_polyfill = BASE64_POLYFILL,
                text_encoder_polyfill = TEXT_ENCODER_POLYFILL,
                crypto_polyfill = CRYPTO_POLYFILL,
                cheerio_polyfill = CHEERIO_POLYFILL,
                code = code,
            );

            ctx.globals().set(
                "__capture_result",
                Function::new(ctx.clone(), move |s: String| {
                    *captured_clone.lock().expect("capture lock poisoned") = Some(s);
                })?,
            )?;

            ctx.eval::<(), _>(script).catch(&ctx).map_err(|e| {
                rquickjs::Error::new_from_js_message("plugin", "js", e.to_string())
            })?;

            Ok(())
        })
        .await;

    eval_result.map_err(|e| e.to_string())?;
    qjs_rt.idle().await;

    let result = captured
        .lock()
        .expect("capture lock poisoned")
        .take()
        .unwrap_or_else(|| "[]".to_string());
    Ok(result)
}

fn native_fetch(
    client: &Arc<dyn PluginHttpClient>,
    url: String,
    method: String,
    headers_json: String,
    body: Option<String>,
    follow_redirects: bool,
) -> String {
    let headers: HashMap<String, String> = serde_json::from_str(&headers_json).unwrap_or_default();
    let response = client.fetch(PluginHttpRequest {
        method,
        url,
        headers,
        body,
        follow_redirects,
    });
    format!(
        "{{\"ok\":{},\"status\":{},\"body\":{}}}",
        response.ok,
        response.status,
        serde_json::to_string(&response.body).unwrap_or_else(|_| "\"\"".into())
    )
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(hex: &str) -> Vec<u8> {
    let clean: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (0..clean.len() / 2)
        .filter_map(|i| u8::from_str_radix(clean.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

fn register_host_functions(
    ctx: &Ctx<'_>,
    dom: &Rc<DomBridge>,
    client: Arc<dyn PluginHttpClient>,
) -> rquickjs::Result<()> {
    ctx.globals().set(
        "console",
        rquickjs::Object::new(ctx.clone()).and_then(|obj| {
            obj.set(
                "log",
                Function::new(ctx.clone(), |msg: String| log::debug!("[plugin] {msg}"))?,
            )?;
            obj.set(
                "warn",
                Function::new(ctx.clone(), |msg: String| log::warn!("[plugin] {msg}"))?,
            )?;
            obj.set(
                "error",
                Function::new(ctx.clone(), |msg: String| log::warn!("[plugin] {msg}"))?,
            )?;
            obj.set(
                "info",
                Function::new(ctx.clone(), |msg: String| log::debug!("[plugin] {msg}"))?,
            )?;
            obj.set(
                "debug",
                Function::new(ctx.clone(), |msg: String| log::debug!("[plugin] {msg}"))?,
            )?;
            Ok(obj)
        })?,
    )?;

    ctx.globals().set(
        "__native_fetch",
        Function::new(
            ctx.clone(),
            move |url: String,
                  method: String,
                  headers_json: String,
                  body: Option<String>,
                  follow_redirects: bool| {
                native_fetch(&client, url, method, headers_json, body, follow_redirects)
            },
        )?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_load",
        Function::new(ctx.clone(), move |html: String| d.load(html))?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_select",
        Function::new(ctx.clone(), move |doc_id: String, selector: String| {
            d.select(doc_id, selector)
        })?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_find",
        Function::new(
            ctx.clone(),
            move |doc_id: String, element_id: String, selector: String| {
                d.find(doc_id, element_id, selector)
            },
        )?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_text",
        Function::new(ctx.clone(), move |_doc_id: String, element_ids: String| {
            d.text(element_ids)
        })?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_html",
        Function::new(ctx.clone(), move |doc_id: String, element_id: String| {
            d.html(doc_id, element_id)
        })?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_inner_html",
        Function::new(ctx.clone(), move |_doc_id: String, element_id: String| {
            d.inner_html(element_id)
        })?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_attr",
        Function::new(
            ctx.clone(),
            move |_doc_id: String, element_id: String, attr_name: String| {
                d.attr(element_id, attr_name)
            },
        )?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_next",
        Function::new(ctx.clone(), move |doc_id: String, element_id: String| {
            d.next(doc_id, element_id)
        })?,
    )?;

    let d = dom.clone();
    ctx.globals().set(
        "__cheerio_prev",
        Function::new(ctx.clone(), move |doc_id: String, element_id: String| {
            d.prev(doc_id, element_id)
        })?,
    )?;

    ctx.globals().set(
        "__crypto_get_random_values_hex",
        Function::new(ctx.clone(), |len: usize| {
            to_hex(&crypto_bridge::random_bytes(len))
        })?,
    )?;

    ctx.globals().set(
        "__crypto_digest_hex_raw",
        Function::new(ctx.clone(), |algorithm: String, data_hex: String| {
            crypto_bridge::digest(&algorithm, &from_hex(&data_hex))
                .map(|bytes| to_hex(&bytes))
                .unwrap_or_default()
        })?,
    )?;

    ctx.globals().set(
        "__crypto_hmac_hex_raw",
        Function::new(
            ctx.clone(),
            |algorithm: String, key_hex: String, data_hex: String| {
                crypto_bridge::hmac(&algorithm, &from_hex(&key_hex), &from_hex(&data_hex))
                    .map(|bytes| to_hex(&bytes))
                    .unwrap_or_default()
            },
        )?,
    )?;

    ctx.globals().set(
        "__crypto_pbkdf2_hex",
        Function::new(
            ctx.clone(),
            |password_hex: String,
             salt_hex: String,
             iterations: u32,
             key_size_bits: u32,
             algorithm: String| {
                crypto_bridge::pbkdf2(
                    &from_hex(&password_hex),
                    &from_hex(&salt_hex),
                    iterations,
                    key_size_bits,
                    &algorithm,
                )
                .map(|bytes| to_hex(&bytes))
                .unwrap_or_default()
            },
        )?,
    )?;

    ctx.globals().set(
        "__crypto_aes_encrypt_hex",
        Function::new(
            ctx.clone(),
            |mode: String, key_hex: String, iv_hex: String, data_hex: String| {
                crypto_bridge::aes_encrypt(
                    &mode,
                    &from_hex(&key_hex),
                    &from_hex(&iv_hex),
                    &from_hex(&data_hex),
                )
                .map(|bytes| to_hex(&bytes))
                .unwrap_or_default()
            },
        )?,
    )?;

    ctx.globals().set(
        "__crypto_aes_decrypt_hex",
        Function::new(
            ctx.clone(),
            |mode: String, key_hex: String, iv_hex: String, data_hex: String| {
                crypto_bridge::aes_decrypt(
                    &mode,
                    &from_hex(&key_hex),
                    &from_hex(&iv_hex),
                    &from_hex(&data_hex),
                )
                .map(|bytes| to_hex(&bytes))
                .unwrap_or_default()
            },
        )?,
    )?;

    ctx.globals().set(
        "__crypto_utf8_to_hex",
        Function::new(ctx.clone(), |text: String| to_hex(text.as_bytes()))?,
    )?;

    ctx.globals().set(
        "__crypto_hex_to_utf8",
        Function::new(ctx.clone(), |hex: String| {
            String::from_utf8_lossy(&from_hex(&hex)).into_owned()
        })?,
    )?;

    ctx.globals().set(
        "__crypto_sign_hex",
        Function::new(
            ctx.clone(),
            |algorithm: String, key_hex: String, data_hex: String| {
                crypto_bridge::sign(&algorithm, &from_hex(&key_hex), &from_hex(&data_hex))
                    .map(|bytes| to_hex(&bytes))
                    .unwrap_or_default()
            },
        )?,
    )?;

    ctx.globals().set(
        "__crypto_verify_hex",
        Function::new(
            ctx.clone(),
            |algorithm: String, key_hex: String, signature_hex: String, data_hex: String| {
                crypto_bridge::verify(
                    &algorithm,
                    &from_hex(&key_hex),
                    &from_hex(&signature_hex),
                    &from_hex(&data_hex),
                )
                .unwrap_or(false)
            },
        )?,
    )?;

    Ok(())
}

const BASE64_POLYFILL: &str = include_str!("polyfills/base64.js");
const TEXT_ENCODER_POLYFILL: &str = include_str!("polyfills/text_encoder.js");
const CRYPTO_POLYFILL: &str = include_str!("polyfills/crypto.js");
const CHEERIO_POLYFILL: &str = include_str!("polyfills/cheerio.js");
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
