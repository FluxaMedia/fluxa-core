use super::host_functions::{
    BASE64_POLYFILL, CHEERIO_POLYFILL, CRYPTO_POLYFILL, TEXT_ENCODER_POLYFILL,
    register_host_functions,
};
use super::{PLUGIN_MEMORY_LIMIT, PLUGIN_TIMEOUT_SECS, PluginHttpClient};
use crate::plugin_runtime::dom_bridge::DomBridge;
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Function};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) async fn run(
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
                    if let Ok(mut captured) = captured_clone.lock() {
                        *captured = Some(s);
                    }
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
        .ok()
        .and_then(|mut captured| captured.take())
        .unwrap_or_else(|| "[]".to_string());
    Ok(result)
}
