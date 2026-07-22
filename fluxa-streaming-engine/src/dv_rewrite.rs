use dolby_vision::rpu::dovi_rpu::DoviRpu;
use dolby_vision::rpu::extension_metadata::blocks::ExtMetadataBlock;
use serde::Deserialize;

mod dvcc;
mod hls_urls;
mod stats;

// Startup self-test
//
// Verifies that libdovi is linked and the error path works without panicking.
// Returns `true` if the library responds correctly to an invalid RPU payload
// (expected: returns Err, not a panic or incorrect Ok).
// Cached per-process — call it at startup and reuse the result.
pub(crate) fn dv_rpu_self_test() -> bool {
    let dummy = [0xFFu8; 16]; // definitely not a valid RPU
    DoviRpu::parse_unspec62_nalu(&dummy).is_err()
}

// Set to true by stream_auto_detect when it strips a P5 CID≠1 (IPTPQc2) stream.
// Read by Kotlin in onVideoInputFormatChanged to activate the IPTPQc2 → SDR shader.
static DV_LAST_AUTO_DETECT_IPTPQC2: AtomicBool = AtomicBool::new(false);
static LAST_L1_VALID: AtomicBool = AtomicBool::new(false);
static LAST_L1_MIN_PQ: AtomicU32 = AtomicU32::new(0);
static LAST_L1_MAX_PQ: AtomicU32 = AtomicU32::new(2048);
static LAST_L1_AVG_PQ: AtomicU32 = AtomicU32::new(1024);

pub(crate) fn dv_auto_detect_was_iptpqc2() -> bool {
    DV_LAST_AUTO_DETECT_IPTPQC2.load(Ordering::Relaxed)
}

pub(crate) fn dv_get_current_l1_json() -> String {
    if !LAST_L1_VALID.load(Ordering::Relaxed) {
        return "{\"available\":false}".to_string();
    }
    format!(
        "{{\"available\":true,\"min_pq\":{},\"max_pq\":{},\"avg_pq\":{}}}",
        LAST_L1_MIN_PQ.load(Ordering::Relaxed),
        LAST_L1_MAX_PQ.load(Ordering::Relaxed),
        LAST_L1_AVG_PQ.load(Ordering::Relaxed),
    )
}

fn store_l1_from_rpu(rpu: &DoviRpu) {
    let Some(dm) = &rpu.vdr_dm_data else { return };
    let Some(ExtMetadataBlock::Level1(l1)) = dm.get_block(1) else {
        return;
    };
    LAST_L1_MIN_PQ.store(l1.min_pq as u32, Ordering::Relaxed);
    LAST_L1_MAX_PQ.store(l1.max_pq as u32, Ordering::Relaxed);
    LAST_L1_AVG_PQ.store(l1.avg_pq as u32, Ordering::Relaxed);
    LAST_L1_VALID.store(true, Ordering::Relaxed);
}

// Synchronous byte-buffer segment rewriter
//
// Used by the Kotlin OkHttp interceptor to convert HLS segments (fMP4 .m4s or
// TS .ts) in-place.  Detects framing from the first bytes, routes to the
// appropriate rewriter, and returns the processed bytes.

pub(crate) fn dv_rewrite_segment_bytes(
    data: &[u8],
    rpu_mode: u8,
    zero_level5: bool,
    remove_hdr10plus: bool,
) -> Vec<u8> {
    if data.len() < 4 {
        return data.to_vec();
    }
    stats::reset();

    let is_annexb = (data[0] == 0 && data[1] == 0 && data[2] == 1)
        || (data[0] == 0 && data[1] == 0 && data[2] == 0 && data[3] == 1);
    let is_ebml = data[0] == 0x1A && data[1] == 0x45 && data[2] == 0xDF && data[3] == 0xA3;

    if is_ebml {
        let mut rewriter = MkvRpuRewriter::new(rpu_mode, zero_level5);
        let mut out = rewriter.process(data);
        out.extend(rewriter.flush());
        out
    } else if is_annexb {
        let mut state = NalRewriteState::new_rpu_convert(rpu_mode, zero_level5, remove_hdr10plus);
        let mut out = state.process(data);
        let (conv, fail) = state.rpu_stats();
        out.extend(state.flush());
        stats::add(conv, fail, 0);
        eprintln!("[fluxa/rpu_convert_sync] annexb rpu_converted={conv} rpu_failed={fail}");
        out
    } else {
        // fMP4 (.m4s segments, HLS)
        let mut rewriter = FMp4NalRewriter::new(rpu_mode, zero_level5, remove_hdr10plus);
        let mut out = rewriter.process(data);
        out.extend(rewriter.flush()); // flush calls dv_stats_add internally
        out
    }
}

// Per-stream conversion stats
//
// Global relaxed atomics — diagnostic only, no ordering guarantees needed.
// Reset at the start of each rpu_convert stream; read at any time from JNI.

pub(crate) fn dv_get_stream_stats_json() -> String {
    stats::as_json()
}
use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::local_stream::{
    build_proxy_client, local_stream_servers, next_local_stream_id, parse_request,
    send_upstream_request, write_simple_response, LocalStreamConfig, LocalStreamHandle,
};

// Public config
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DvRewriteConfig {
    /// "dvcc_strip"    — mangle DVCC/DVHE fourcc in the container header (MKV/MP4 → HDR10)
    /// "rpu_convert"   — rewrite UNSPEC62 RPU NALs in an Annex-B HEVC stream (P7 → P8)
    /// "hdr10plus_strip" — strip HDR10+ SEI NALs from an Annex-B HEVC stream
    /// "auto_detect"   — read dvcC box, apply Kodi-equivalent decision logic automatically
    pub action: String,

    /// libdovi convert mode for rpu_convert / auto_detect (2 = Profile 8, default).
    #[serde(default = "default_rpu_mode")]
    pub rpu_mode: u8,

    /// Device has a hardware Dolby Vision decoder (from MediaCodecList).
    #[serde(default)]
    pub device_has_dv_decoder: bool,

    /// Device display reports HDR_TYPE_DOLBY_VISION support.
    #[serde(default)]
    pub device_has_dv_display: bool,

    /// Zero out Level 5 active-area offsets in every RPU (mirrors Kodi SetDoviZeroLevel5).
    #[serde(default)]
    pub zero_level5: bool,

    /// Strip HDR10+ SEI NALs alongside DV RPU processing (mirrors Kodi removeHdr10Plus).
    #[serde(default)]
    pub remove_hdr10plus: bool,

    /// Fallback mode: "auto" | "off"  (mirrors DolbyVisionFallbackMode).
    #[serde(default = "default_fallback_mode")]
    pub fallback_mode: String,
}

fn default_rpu_mode() -> u8 {
    2
}

fn default_fallback_mode() -> String {
    "auto".to_string()
}

// Entry point
pub(crate) fn start_dv_rewrite_local_stream_server(
    target_url: &str,
    headers_json: &str,
    dv_config_json: &str,
    preferred_port: i32,
) -> Option<String> {
    let headers = serde_json::from_str::<HashMap<String, String>>(headers_json).unwrap_or_default();
    let dv_config = Arc::new(serde_json::from_str::<DvRewriteConfig>(dv_config_json).ok()?);

    let id = next_local_stream_id();
    let bind_port = preferred_port.clamp(0, u16::MAX as i32) as u16;
    let listener = TcpListener::bind(("0.0.0.0", bind_port)).ok()?;
    let port = listener.local_addr().ok()?.port();
    listener.set_nonblocking(true).ok()?;

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let config = LocalStreamConfig {
        id: id.clone(),
        target_url: target_url.to_string(),
        headers,
        client: Arc::new(build_proxy_client()),
        active_connections: Arc::new(AtomicUsize::new(0)),
        port,
    };

    let thread = thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let cfg = config.clone();
                    let dv = dv_config.clone();
                    thread::spawn(move || handle_dv_stream(stream, cfg, &dv));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(40));
                }
                Err(_) => break,
            }
        }
    });

    local_stream_servers().lock().ok()?.insert(
        id.clone(),
        LocalStreamHandle {
            stop,
            thread: Some(thread),
        },
    );

    serde_json::to_string(&json!({
        "id": id.clone(),
        "url": format!("http://127.0.0.1:{port}/stream/{id}"),
        "port": port
    }))
    .ok()
}

fn handle_dv_stream(mut stream: TcpStream, config: LocalStreamConfig, dv: &DvRewriteConfig) {
    let Some(request) = parse_request(&mut stream) else {
        write_simple_response(&mut stream, "400 Bad Request");
        return;
    };
    if !request.path.starts_with(&format!("/stream/{}", config.id)) {
        write_simple_response(&mut stream, "404 Not Found");
        return;
    }
    if request.method != "GET" && request.method != "HEAD" {
        write_simple_response(&mut stream, "405 Method Not Allowed");
        return;
    }

    if dv.action == "hls_rpu_convert" {
        handle_hls_rpu_convert(stream, config, dv, &request);
        return;
    }

    let mut response =
        match send_upstream_request(&config.client, &config, &request.method, &request.headers) {
            Ok(r) => r,
            Err(_) => {
                write_simple_response(&mut stream, "502 Bad Gateway");
                return;
            }
        };

    let status = response.status();
    let _ = write!(
        stream,
        "HTTP/1.1 {} {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("OK")
    );
    for name in [
        "content-type",
        "content-range",
        "accept-ranges",
        "etag",
        "last-modified",
    ] {
        if let Some(v) = response.headers().get(name).and_then(|v| v.to_str().ok()) {
            let _ = write!(stream, "{name}: {v}\r\n");
        }
    }
    let _ = write!(stream, "Connection: close\r\n\r\n");

    if request.method == "HEAD" {
        return;
    }

    match dv.action.as_str() {
        "dvcc_strip" => stream_dvcc_strip(&mut response, &mut stream),
        "rpu_convert" => stream_rpu_convert(
            &mut response,
            &mut stream,
            dv.rpu_mode,
            dv.zero_level5,
            dv.remove_hdr10plus,
        ),
        "hdr10plus_strip" => stream_hdr10plus_strip(&mut response, &mut stream),
        "auto_detect" => stream_auto_detect(&mut response, &mut stream, dv),
        _ => {
            let _ = std::io::copy(&mut response, &mut stream);
        }
    }
}

fn handle_hls_rpu_convert(
    mut downstream: TcpStream,
    config: LocalStreamConfig,
    dv: &DvRewriteConfig,
    request: &crate::local_stream::ParsedLocalRequest,
) {
    let seg_prefix = format!("/stream/{}/seg", config.id);
    let stream_path = format!("/stream/{}", config.id);

    if request.path.starts_with(&seg_prefix) {
        let query = request.path[seg_prefix.len()..].trim_start_matches('?');
        let seg_url = query
            .split('&')
            .find_map(|p| p.strip_prefix("u="))
            .map(hls_urls::percent_decode)
            .unwrap_or_default();

        if seg_url.is_empty() {
            write_simple_response(&mut downstream, "400 Bad Request");
            return;
        }

        let url_lower = seg_url.to_ascii_lowercase();
        if url_lower.contains(".m3u8") {
            serve_hls_manifest_rewritten(&mut downstream, &config, dv, &seg_url, request);
        } else {
            serve_hls_segment_rpu_convert(&mut downstream, &config, dv, &seg_url, request);
        }
    } else if request.path == stream_path || request.path.starts_with(&format!("{}?", stream_path))
    {
        serve_hls_manifest_rewritten(
            &mut downstream,
            &config,
            dv,
            &config.target_url.clone(),
            request,
        );
    } else {
        write_simple_response(&mut downstream, "404 Not Found");
    }
}

fn serve_hls_manifest_rewritten(
    downstream: &mut TcpStream,
    config: &LocalStreamConfig,
    _dv: &DvRewriteConfig,
    manifest_url: &str,
    request: &crate::local_stream::ParsedLocalRequest,
) {
    let upstream = match hls_urls::fetch(
        &config.client,
        manifest_url,
        &config.headers,
        &request.headers,
    ) {
        Ok(r) => r,
        Err(_) => {
            write_simple_response(downstream, "502 Bad Gateway");
            return;
        }
    };
    let ct = upstream
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/vnd.apple.mpegurl")
        .to_owned();
    let body = match upstream.text() {
        Ok(t) => t,
        Err(_) => {
            write_simple_response(downstream, "502 Bad Gateway");
            return;
        }
    };

    let proxy_seg_base = format!(
        "http://127.0.0.1:{}/stream/{}/seg?u=",
        config.port, config.id
    );
    let rewritten = hls_urls::rewrite_manifest(&body, manifest_url, &proxy_seg_base);
    let bytes = rewritten.as_bytes();

    let _ = write!(
        downstream,
        "HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    if request.method != "HEAD" {
        let _ = downstream.write_all(bytes);
    }
}

fn serve_hls_segment_rpu_convert(
    downstream: &mut TcpStream,
    config: &LocalStreamConfig,
    dv: &DvRewriteConfig,
    seg_url: &str,
    request: &crate::local_stream::ParsedLocalRequest,
) {
    let mut response =
        match hls_urls::fetch(&config.client, seg_url, &config.headers, &request.headers) {
            Ok(r) => r,
            Err(_) => {
                write_simple_response(downstream, "502 Bad Gateway");
                return;
            }
        };

    let status = response.status();
    let _ = write!(
        downstream,
        "HTTP/1.1 {} {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("OK")
    );
    for name in ["content-type", "content-range", "accept-ranges"] {
        if let Some(v) = response.headers().get(name).and_then(|v| v.to_str().ok()) {
            let _ = write!(downstream, "{name}: {v}\r\n");
        }
    }
    let _ = write!(downstream, "Connection: close\r\n\r\n");

    if request.method != "HEAD" {
        stream_rpu_convert(
            &mut response,
            downstream,
            dv.rpu_mode,
            dv.zero_level5,
            dv.remove_hdr10plus,
        );
    }
}

// DVCC strip (MKV / MP4 container)
//
// Searches the first 64 KiB of the stream for the DVCC or DVHE ISO-BMFF box
// type fourcc and overwrites it with "XXXX".  ExoPlayer's MatroskaExtractor
// and MP4Extractor both key off this fourcc to set VIDEO_DOLBY_VISION; after
// mangling they fall back to VIDEO_H265 and decode the base layer as HDR10.

fn stream_dvcc_strip(upstream: &mut reqwest::blocking::Response, downstream: &mut TcpStream) {
    const SCAN_WINDOW: usize = 65536;

    let raw_range_header = upstream
        .headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let (file_offset, range_source): (u64, &str) = match raw_range_header
        .as_deref()
        .and_then(dvcc::parse_content_range_start)
    {
        Some(offset) => (offset, "content_range_header"),
        None if raw_range_header.is_some() => (0, "content_range_parse_error_assumed_zero"),
        None => (0, "missing_assumed_zero"),
    };

    eprintln!(
        "[fluxa/dvcc_strip] range_header={range_source} file_offset={file_offset} \
         scan_limit={SCAN_WINDOW}"
    );

    if file_offset >= SCAN_WINDOW as u64 {
        eprintln!(
            "[fluxa/dvcc_strip] planned=dvcc_strip applied=false \
             reason=range_past_scan_window"
        );
        let _ = std::io::copy(upstream, downstream);
        return;
    }

    let patch_region = (SCAN_WINDOW as u64 - file_offset) as usize;
    let mut header_buf: Vec<u8> = Vec::with_capacity(patch_region);
    let mut tmp = [0u8; 8192];

    while header_buf.len() < patch_region {
        let n = upstream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        header_buf.extend_from_slice(&tmp[..n]);
    }

    let patch_count = dvcc::apply_patch_at_offset(&mut header_buf, file_offset, SCAN_WINDOW);
    let box_found = patch_count > 0;

    if box_found {
        eprintln!(
            "[fluxa/dvcc_strip] box_found=true patch_count={patch_count} \
             file_offset={file_offset} patch_region={patch_region} \
             scan_limit={SCAN_WINDOW} patch_scope=header_only"
        );
    } else {
        eprintln!(
            "[fluxa/dvcc_strip] planned=dvcc_strip applied=false \
             reason=no_dv_config_found_in_scan_window \
             file_offset={file_offset} scan_limit={SCAN_WINDOW}"
        );
    }

    if downstream.write_all(&header_buf).is_err() {
        return;
    }
    let _ = std::io::copy(upstream, downstream);
}

/// Replace every known Dolby Vision fourcc occurrence with "XXXX" (same length).
/// Returns the number of four-character codes that were replaced.
/// Parse the start offset from an HTTP `Content-Range: bytes START-END/TOTAL` header.
// dvcC box parser
#[derive(Debug, Clone, Copy)]
struct DvContainerInfo {
    profile: u8,
    compat_id: u8,
}

impl DvContainerInfo {
    /// Mirrors Kodi's `notHasHDR10fallback` check
    /// (DVDVideoCodecAndroidMediaCodec.cpp:543-544 and 698-700):
    ///
    ///   Profiles 4 and 5 are single-layer DV-only (no HEVC HDR10 base layer).
    ///   Profile 10 with dv_bl_signal_compatibility_id 0, 2, or 3 is also DV-only.
    fn not_has_hdr10_fallback(self) -> bool {
        // P5 CID=1 has an HDR10 base layer and can fall back to HDR10 — same exception
        // as the HLS manifest rewriter. All other P5 variants are DV-only.
        self.profile == 4
            || (self.profile == 5 && self.compat_id != 1)
            || (self.profile == 10 && matches!(self.compat_id, 0 | 2 | 3))
    }
}

/// Scan `data` for a `dvcC` ISO-BMFF box and return the parsed DV profile info.
fn scan_dvcc_info(data: &[u8]) -> Option<DvContainerInfo> {
    for i in 0..data.len().saturating_sub(8) {
        if data[i..i + 4] == *b"dvcC" {
            return parse_dvcc_payload(&data[i + 4..]);
        }
    }
    None
}

/// Parse a DOVIDecoderConfigurationRecord starting immediately after the "dvcC"
/// fourcc (i.e. `data[0]` = dv_version_major).
///
/// Bit layout (ISOBMFF Dolby Vision spec, 8-byte record):
///   byte[0]        dv_version_major
///   byte[1]        dv_version_minor
///   byte[2][7:1]   dv_profile  (7 bits)
///   byte[2][0]     dv_level high bit
///   byte[3][7:3]   dv_level low 5 bits
///   byte[3][2:0]   rpu/el/bl_present_flags
///   byte[4][7:4]   dv_bl_signal_compatibility_id  (4 bits)
fn parse_dvcc_payload(data: &[u8]) -> Option<DvContainerInfo> {
    if data.len() < 5 {
        return None;
    }
    let profile = (data[2] >> 1) & 0x7F;
    let compat_id = (data[4] >> 4) & 0x0F;
    Some(DvContainerInfo { profile, compat_id })
}

// Auto-detect (Kodi-equivalent container analysis)
//
// Implements Kodi's exact decision logic from
// DVDVideoCodecAndroidMediaCodec.cpp lines 543-546 and 698-700, but operating
// on the raw byte stream of the container rather than on FFmpeg demuxer hints.
//
// Decision:
//   if device_has_dv_decoder && (device_has_dv_display || not_has_hdr10_fallback)
//       → pass through (device plays DV natively, no rewrite needed)
//   else if !not_has_hdr10_fallback
//       → mangle DVCC fourcc (stream plays as HDR10 via base layer)
//   else
//       → pass through unchanged (DV-only profile, cannot fall back)
//
// fallback_mode override:
//   "off"  → always pass through
//   "auto" → Kodi device-capability logic (default)

fn stream_auto_detect(
    upstream: &mut reqwest::blocking::Response,
    downstream: &mut TcpStream,
    config: &DvRewriteConfig,
) {
    DV_LAST_AUTO_DETECT_IPTPQC2.store(false, Ordering::Relaxed);
    const SCAN_WINDOW: usize = 65536;

    let raw_range_header = upstream
        .headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let (file_offset, range_source): (u64, &str) = match raw_range_header
        .as_deref()
        .and_then(dvcc::parse_content_range_start)
    {
        Some(offset) => (offset, "content_range_header"),
        None if raw_range_header.is_some() => (0, "content_range_parse_error_assumed_zero"),
        None => (0, "missing_assumed_zero"),
    };

    eprintln!(
        "[fluxa/auto_detect] range_header={range_source} file_offset={file_offset} \
         scan_limit={SCAN_WINDOW} fallback_mode={}",
        config.fallback_mode
    );

    if file_offset >= SCAN_WINDOW as u64 {
        eprintln!(
            "[fluxa/auto_detect] applied=false reason=range_past_scan_window \
             file_offset={file_offset}"
        );
        let _ = std::io::copy(upstream, downstream);
        return;
    }

    let patch_region = (SCAN_WINDOW as u64 - file_offset) as usize;
    let mut header_buf: Vec<u8> = Vec::with_capacity(patch_region);
    let mut tmp = [0u8; 8192];
    while header_buf.len() < patch_region {
        let n = upstream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        header_buf.extend_from_slice(&tmp[..n]);
    }

    let should_strip = match dvcc::scan_info(&header_buf) {
        None => {
            eprintln!("[fluxa/auto_detect] decision=pass_through reason=no_dvcC_in_scan_window");
            false
        }
        Some(info) => {
            let not_has_fallback = info.not_has_hdr10_fallback();
            let device_supports_dv =
                config.device_has_dv_decoder && (config.device_has_dv_display || not_has_fallback);

            // P5 has a HEVC base layer (IPTPQc2-encoded) with no HDR10 fallback.
            // Strip its DVCC box so ExoPlayer decodes it as HEVC on non-DV devices.
            // DV_LAST_AUTO_DETECT_IPTPQC2 is set so Kotlin can activate the IPTPQc2 shader.
            let strip = match config.fallback_mode.as_str() {
                "off" => false,
                _ => !device_supports_dv && (!not_has_fallback || info.profile == 5),
            };

            let is_p5_iptpqc2 = strip && info.profile == 5 && info.compat_id != 1;
            DV_LAST_AUTO_DETECT_IPTPQC2.store(is_p5_iptpqc2, Ordering::Relaxed);

            let decision = if strip {
                if is_p5_iptpqc2 {
                    "dvcc_strip_p5_iptpqc2"
                } else {
                    "dvcc_strip_to_hdr10"
                }
            } else if device_supports_dv {
                "keep_dv_native"
            } else {
                "pass_through_no_fallback"
            };
            eprintln!(
                "[fluxa/auto_detect] dv_profile={} compat_id={} not_has_fallback={} \
                 device_has_dv_decoder={} device_has_dv_display={} decision={decision} iptpqc2={is_p5_iptpqc2}",
                info.profile,
                info.compat_id,
                not_has_fallback,
                config.device_has_dv_decoder,
                config.device_has_dv_display,
            );
            strip
        }
    };

    if should_strip {
        let patch_count = dvcc::apply_patch_at_offset(&mut header_buf, file_offset, SCAN_WINDOW);
        eprintln!("[fluxa/auto_detect] dvcc_patch_count={patch_count}");
    }

    if downstream.write_all(&header_buf).is_err() {
        return;
    }
    let _ = std::io::copy(upstream, downstream);
}

// RPU convert (Annex-B HEVC bitstream)
//
// Parses the raw byte stream as HEVC Annex-B start-code-delimited NAL units.
// For every UNSPEC62 (RPU) NAL, runs libdovi convert_with_mode to rewrite to
// the requested profile.  Optionally zeros Level 5 active-area metadata
// (mirrors Kodi's SetDoviZeroLevel5) and strips HDR10+ SEI NALs
// (mirrors Kodi's removeHdr10Plus).

fn stream_rpu_convert(
    upstream: &mut reqwest::blocking::Response,
    downstream: &mut TcpStream,
    rpu_mode: u8,
    zero_level5: bool,
    remove_hdr10plus: bool,
) {
    // Probe the first 8 bytes to detect whether this is an Annex-B HEVC bitstream or an
    // ISO-BMFF (fMP4/MP4) container. HLS segments and direct MP4 files use length-prefixed
    // NAL units, not Annex-B start codes. Applying the NAL rewriter to fMP4 would be a
    // silent no-op that leaves DV RPU NALs in the stream; fall back to DVCC strip instead.
    stats::reset();
    let mut probe = [0u8; 8];
    let n = upstream.read(&mut probe).unwrap_or(0);
    if n < 3 {
        let _ = downstream.write_all(&probe[..n]);
        return;
    }
    let is_annexb = (probe[0] == 0 && probe[1] == 0 && probe[2] == 1)
        || (n >= 4 && probe[0] == 0 && probe[1] == 0 && probe[2] == 0 && probe[3] == 1);

    // EBML magic: 0x1A 0x45 0xDF 0xA3
    let is_ebml =
        n >= 4 && probe[0] == 0x1A && probe[1] == 0x45 && probe[2] == 0xDF && probe[3] == 0xA3;

    if is_ebml {
        stream_rpu_convert_mkv(&probe[..n], upstream, downstream, rpu_mode, zero_level5);
        return;
    }

    if !is_annexb {
        stream_rpu_convert_fmp4(
            &probe[..n],
            upstream,
            downstream,
            rpu_mode,
            zero_level5,
            remove_hdr10plus,
        );
        return;
    }

    // Annex-B confirmed — run NAL rewrite, feeding probe bytes as the first chunk.
    let mut state = NalRewriteState::new_rpu_convert(rpu_mode, zero_level5, remove_hdr10plus);
    let out = state.process(&probe[..n]);
    if downstream.write_all(&out).is_err() {
        return;
    }
    let mut buf = [0u8; 65536];
    loop {
        let r = upstream.read(&mut buf).unwrap_or(0);
        if r == 0 {
            let (conv, fail) = state.rpu_stats();
            let tail = state.flush();
            let _ = downstream.write_all(&tail);
            stats::add(conv, fail, 0);
            eprintln!("[fluxa/rpu_convert] stream_end rpu_converted={conv} rpu_failed={fail}");
            break;
        }
        let out = state.process(&buf[..r]);
        if downstream.write_all(&out).is_err() {
            break;
        }
    }
}

// fMP4 / length-delimited NAL rewriter
//
// HLS delivers video as fragmented MP4 (fMP4) segments. Each segment is a
// sequence of ISO-BMFF boxes (typically moof + mdat). Inside mdat, HEVC
// samples are length-delimited: a 4-byte big-endian size prefix followed by
// the raw NAL payload — no Annex-B start codes.
//
// This rewriter parses the box stream as it arrives (streaming, no full
// buffering), forwards non-mdat boxes unchanged, and for mdat boxes scans the
// length-delimited NAL units:
//   RPU NALs (type 62)           → converted via libdovi
//   EL NALs (layer_id > 0, !RPU) → dropped  (not needed for DV8.1 single-layer)
//   BL / other NALs              → forwarded unchanged
//
// The mdat box-size field is updated in the output to reflect any dropped NALs.

fn stream_rpu_convert_fmp4(
    probe: &[u8],
    upstream: &mut reqwest::blocking::Response,
    downstream: &mut TcpStream,
    rpu_mode: u8,
    zero_level5: bool,
    remove_hdr10plus: bool,
) {
    eprintln!("[fluxa/rpu_convert] fmp4 detected — length-delimited NAL rewriter");
    let mut rewriter = FMp4NalRewriter::new(rpu_mode, zero_level5, remove_hdr10plus);
    let init = rewriter.process(probe);
    if !init.is_empty() && downstream.write_all(&init).is_err() {
        return;
    }
    let mut buf = [0u8; 65536];
    loop {
        let n = upstream.read(&mut buf).unwrap_or(0);
        if n == 0 {
            let tail = rewriter.flush();
            let _ = downstream.write_all(&tail);
            break;
        }
        let out = rewriter.process(&buf[..n]);
        if downstream.write_all(&out).is_err() {
            break;
        }
    }
}

enum FMp4State {
    /// Waiting to accumulate an 8-byte ISO-BMFF box header.
    Header,
    /// Forwarding a non-mdat box's content verbatim.
    Forward { remaining: u64 },
    /// Accumulating mdat payload before NAL processing (box size is known).
    Mdat { buf: Vec<u8>, remaining: u64 },
    /// Accumulating mdat payload that extends to EOF (box size field = 0).
    MdatEof { buf: Vec<u8> },
}

struct FMp4NalRewriter {
    state: FMp4State,
    header_buf: Vec<u8>,
    rpu_mode: u8,
    zero_level5: bool,
    remove_hdr10plus: bool,
}

impl FMp4NalRewriter {
    fn new(rpu_mode: u8, zero_level5: bool, remove_hdr10plus: bool) -> Self {
        Self {
            state: FMp4State::Header,
            header_buf: Vec::with_capacity(8),
            rpu_mode,
            zero_level5,
            remove_hdr10plus,
        }
    }

    fn process(&mut self, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut pos = 0;

        while pos < input.len() {
            // Take ownership of state to avoid borrow issues in the match arms.
            let state = std::mem::replace(&mut self.state, FMp4State::Header);
            match state {
                FMp4State::Header => {
                    let needed = 8usize.saturating_sub(self.header_buf.len());
                    let take = needed.min(input.len() - pos);
                    self.header_buf.extend_from_slice(&input[pos..pos + take]);
                    pos += take;

                    if self.header_buf.len() < 8 {
                        // Stay in Header state (already set by replace above).
                        break;
                    }

                    let size_field = u32::from_be_bytes([
                        self.header_buf[0],
                        self.header_buf[1],
                        self.header_buf[2],
                        self.header_buf[3],
                    ]);
                    let is_mdat = self.header_buf[4..8] == *b"mdat";
                    let header = std::mem::take(&mut self.header_buf);

                    self.state = if is_mdat {
                        match size_field {
                            // size=0: mdat extends to EOF
                            0 => FMp4State::MdatEof { buf: Vec::new() },
                            // size=1: 64-bit extended size — rare, treat as opaque forward
                            1 => {
                                out.extend_from_slice(&header);
                                FMp4State::Forward {
                                    remaining: u64::MAX,
                                }
                            }
                            n => {
                                let content = (n as u64).saturating_sub(8);
                                if content == 0 {
                                    // Empty mdat: write header unchanged, return to box parsing.
                                    out.extend_from_slice(&header);
                                    FMp4State::Header
                                } else {
                                    // Buffer the mdat payload; write corrected header after processing.
                                    FMp4State::Mdat {
                                        buf: Vec::with_capacity(
                                            content.min(32 * 1024 * 1024) as usize
                                        ),
                                        remaining: content,
                                    }
                                }
                            }
                        }
                    } else {
                        out.extend_from_slice(&header);
                        match size_field {
                            0 | 1 => FMp4State::Forward {
                                remaining: u64::MAX,
                            },
                            n => {
                                let content = (n as u64).saturating_sub(8);
                                if content == 0 {
                                    FMp4State::Header
                                } else {
                                    FMp4State::Forward { remaining: content }
                                }
                            }
                        }
                    };
                }

                FMp4State::Forward { mut remaining } => {
                    let available = (input.len() - pos) as u64;
                    let take = if remaining == u64::MAX {
                        available
                    } else {
                        available.min(remaining)
                    };
                    out.extend_from_slice(&input[pos..pos + take as usize]);
                    pos += take as usize;
                    if remaining != u64::MAX {
                        remaining -= take;
                        self.state = if remaining == 0 {
                            FMp4State::Header
                        } else {
                            FMp4State::Forward { remaining }
                        };
                    } else {
                        self.state = FMp4State::Forward {
                            remaining: u64::MAX,
                        };
                    }
                }

                FMp4State::Mdat {
                    mut buf,
                    mut remaining,
                } => {
                    let available = (input.len() - pos) as u64;
                    let take = available.min(remaining) as usize;
                    buf.extend_from_slice(&input[pos..pos + take]);
                    pos += take;
                    remaining -= take as u64;

                    if remaining == 0 {
                        let original_len = buf.len();
                        let (processed, rpu_count, rpu_fail, el_dropped) =
                            rewrite_length_delimited_nals(
                                &buf,
                                self.rpu_mode,
                                self.zero_level5,
                                self.remove_hdr10plus,
                            );
                        stats::add(rpu_count, rpu_fail, el_dropped);
                        eprintln!(
                            "[fluxa/rpu_convert_fmp4] mdat original_size={original_len} \
                             new_size={} rpu_converted={rpu_count} rpu_failed={rpu_fail} el_dropped={el_dropped}",
                            processed.len()
                        );
                        let new_box_size = (processed.len() + 8) as u32;
                        out.extend_from_slice(&new_box_size.to_be_bytes());
                        out.extend_from_slice(b"mdat");
                        out.extend_from_slice(&processed);
                        self.state = FMp4State::Header;
                    } else {
                        self.state = FMp4State::Mdat { buf, remaining };
                    }
                }

                FMp4State::MdatEof { mut buf } => {
                    buf.extend_from_slice(&input[pos..]);
                    pos = input.len();
                    self.state = FMp4State::MdatEof { buf };
                }
            }
        }

        out
    }

    fn flush(self) -> Vec<u8> {
        let mut out = Vec::new();
        match self.state {
            FMp4State::MdatEof { buf } => {
                let original_len = buf.len();
                let (processed, rpu_count, rpu_fail, el_dropped) = rewrite_length_delimited_nals(
                    &buf,
                    self.rpu_mode,
                    self.zero_level5,
                    self.remove_hdr10plus,
                );
                stats::add(rpu_count, rpu_fail, el_dropped);
                eprintln!(
                    "[fluxa/rpu_convert_fmp4] mdat(eof) original_size={original_len} \
                     new_size={} rpu_converted={rpu_count} rpu_failed={rpu_fail} el_dropped={el_dropped}",
                    processed.len()
                );
                // Preserve size=0 (EOF-scoped) semantics in the output box.
                out.extend_from_slice(&[0, 0, 0, 0]);
                out.extend_from_slice(b"mdat");
                out.extend_from_slice(&processed);
            }
            FMp4State::Header if !self.header_buf.is_empty() => {
                // Incomplete box header at EOF: forward the partial bytes as-is.
                out.extend_from_slice(&self.header_buf);
            }
            _ => {}
        }
        out
    }
}

/// Scan a contiguous slice of length-delimited (4-byte BE prefix) HEVC NAL units
/// and rewrite DV7 RPU/EL NALs for DV8.1 single-layer output.
///
/// Returns `(rewritten_payload, rpu_converted_count, el_dropped_count)`.
/// Returns `(rewritten_payload, rpu_converted, rpu_failed, el_dropped)`.
pub(crate) fn rewrite_length_delimited_nals(
    data: &[u8],
    rpu_mode: u8,
    zero_level5: bool,
    remove_hdr10plus: bool,
) -> (Vec<u8>, u32, u32, u32) {
    let mut out = Vec::with_capacity(data.len());
    let mut rpu_converted = 0u32;
    let mut rpu_failed = 0u32;
    let mut el_dropped = 0u32;
    let mut i = 0;

    while i + 4 <= data.len() {
        let nal_len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let payload_end = i + 4 + nal_len;

        if payload_end > data.len() {
            // Truncated NAL at end of mdat — copy remainder unchanged.
            out.extend_from_slice(&data[i..]);
            break;
        }

        let nal = &data[i + 4..payload_end];
        if nal.len() >= 2 {
            let nal_type = (nal[0] >> 1) & 0x3F;
            // HEVC NAL header: nuh_layer_id lives in bits [8:3] across both header bytes.
            let layer_id = ((nal[0] & 0x01) << 5) | (nal[1] >> 3);

            if nal_type == 62 {
                // UNSPEC62 = DV RPU NAL — convert to target profile.
                match convert_rpu_nal(nal, rpu_mode, zero_level5) {
                    Some(converted) => {
                        out.extend_from_slice(&(converted.len() as u32).to_be_bytes());
                        out.extend_from_slice(&converted);
                        rpu_converted += 1;
                    }
                    None => {
                        // Conversion failed: keep original NAL unchanged.
                        out.extend_from_slice(&data[i..payload_end]);
                        rpu_failed += 1;
                    }
                }
            } else if layer_id > 0 {
                // Enhancement layer NAL — not needed for single-layer DV8.1.
                el_dropped += 1;
            } else if remove_hdr10plus && nal_is_hdr10plus_sei(nal) {
                // Single-pass: strip HDR10+ SEI alongside RPU processing.
                eprintln!("[fluxa/rpu_convert_fmp4] stripped_hdr10plus_sei_nal");
            } else {
                out.extend_from_slice(&data[i..payload_end]);
            }
        } else {
            out.extend_from_slice(&data[i..payload_end]);
        }

        i = payload_end;
    }

    (out, rpu_converted, rpu_failed, el_dropped)
}

// HDR10+ SEI strip (Annex-B HEVC bitstream)
//
// Strips SEI NAL units whose first payload is ITU-T T35 with the HDR10+
// provider signature (country=0xB5, provider=0x003C, oriented=0x0001).
// Mirrors Kodi's CBitstreamConverter::SetRemoveHdr10Plus.

fn stream_hdr10plus_strip(upstream: &mut reqwest::blocking::Response, downstream: &mut TcpStream) {
    run_nal_stream(upstream, downstream, NalRewriteState::new_hdr10plus_strip());
}

fn run_nal_stream(
    upstream: &mut reqwest::blocking::Response,
    downstream: &mut TcpStream,
    mut state: NalRewriteState,
) {
    let mut buf = [0u8; 65536];
    loop {
        let n = upstream.read(&mut buf).unwrap_or(0);
        if n == 0 {
            let tail = state.flush();
            let _ = downstream.write_all(&tail);
            break;
        }
        let out = state.process(&buf[..n]);
        if downstream.write_all(&out).is_err() {
            break;
        }
    }
}

// NAL rewrite state machine
enum NalProcessMode {
    RpuConvert {
        rpu_mode: u8,
        zero_level5: bool,
        remove_hdr10plus: bool,
    },
    Hdr10PlusStrip,
}

struct NalRewriteState {
    pending: Vec<u8>,
    mode: NalProcessMode,
    rpu_converted: u32,
    rpu_failed: u32,
}

impl NalRewriteState {
    /// rpu_convert mode — kept for tests.
    #[cfg(test)]
    fn new(rpu_mode: u8) -> Self {
        Self::new_rpu_convert(rpu_mode, false, false)
    }

    fn new_rpu_convert(rpu_mode: u8, zero_level5: bool, remove_hdr10plus: bool) -> Self {
        Self {
            pending: Vec::with_capacity(65536),
            mode: NalProcessMode::RpuConvert {
                rpu_mode,
                zero_level5,
                remove_hdr10plus,
            },
            rpu_converted: 0,
            rpu_failed: 0,
        }
    }

    fn new_hdr10plus_strip() -> Self {
        Self {
            pending: Vec::with_capacity(65536),
            mode: NalProcessMode::Hdr10PlusStrip,
            rpu_converted: 0,
            rpu_failed: 0,
        }
    }

    fn process(&mut self, input: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(input);
        let positions = find_start_code_positions(&self.pending);
        if positions.len() < 2 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for window in positions.windows(2) {
            let (conv, fail) = emit_nal(&self.pending[window[0]..window[1]], &self.mode, &mut out);
            self.rpu_converted += conv;
            self.rpu_failed += fail;
        }
        let last = *positions.last().unwrap();
        self.pending = self.pending[last..].to_vec();
        out
    }

    fn rpu_stats(&self) -> (u32, u32) {
        (self.rpu_converted, self.rpu_failed)
    }

    fn flush(mut self) -> Vec<u8> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let (conv, fail) = emit_nal(&self.pending, &self.mode, &mut out);
        self.rpu_converted += conv;
        self.rpu_failed += fail;
        out
    }
}

/// Emit one Annex-B NAL unit to `out` and return `(rpu_converted, rpu_failed)`.
fn emit_nal(nal_with_sc: &[u8], mode: &NalProcessMode, out: &mut Vec<u8>) -> (u32, u32) {
    let sc = start_code_len(nal_with_sc);
    let nal = &nal_with_sc[sc..];
    if nal.len() < 2 {
        out.extend_from_slice(nal_with_sc);
        return (0, 0);
    }
    let nal_type = (nal[0] >> 1) & 0x3F;

    match mode {
        NalProcessMode::RpuConvert {
            rpu_mode,
            zero_level5,
            remove_hdr10plus,
        } => {
            // Single-pass: strip HDR10+ SEIs and convert RPU NALs together.
            if *remove_hdr10plus && nal_is_hdr10plus_sei(nal) {
                eprintln!("[fluxa/rpu_convert] stripped_hdr10plus_sei_nal");
                return (0, 0);
            }
            if nal_type == 62 {
                if let Some(converted) = convert_rpu_nal(nal, *rpu_mode, *zero_level5) {
                    out.extend_from_slice(&nal_with_sc[..sc]);
                    out.extend_from_slice(&converted);
                    return (1, 0);
                }
                // Conversion failed: keep original
                out.extend_from_slice(nal_with_sc);
                return (0, 1);
            }
            out.extend_from_slice(nal_with_sc);
            (0, 0)
        }
        NalProcessMode::Hdr10PlusStrip => {
            if nal_is_hdr10plus_sei(nal) {
                eprintln!("[fluxa/hdr10plus_strip] stripped_hdr10plus_sei_nal");
            } else {
                out.extend_from_slice(nal_with_sc);
            }
            (0, 0)
        }
    }
}

fn convert_rpu_nal(nal: &[u8], mode: u8, zero_level5: bool) -> Option<Vec<u8>> {
    let mut rpu = DoviRpu::parse_unspec62_nalu(nal).ok()?;
    store_l1_from_rpu(&rpu);
    rpu.convert_with_mode(mode).ok()?;
    if zero_level5 {
        let _ = rpu.crop();
    }
    rpu.write_hevc_unspec62_nalu().ok()
}

// HDR10+ SEI detector
/// Returns true if `nal` (starting with the 2-byte HEVC NAL header) is a
/// PREFIX_SEI (type 39) or SUFFIX_SEI (type 40) whose first SEI message is
/// an ITU-T T35 user_data_registered payload (type 4) carrying the HDR10+
/// signature: country_code=0xB5, terminal_provider_code=0x003C,
/// terminal_provider_oriented_code=0x0001.
fn nal_is_hdr10plus_sei(nal: &[u8]) -> bool {
    if nal.len() < 9 {
        return false;
    }
    // PREFIX_SEI = 39, SUFFIX_SEI = 40
    let nal_type = (nal[0] >> 1) & 0x3F;
    if nal_type != 39 && nal_type != 40 {
        return false;
    }
    // After the 2-byte HEVC NAL header, parse the variable-length SEI payload type.
    let mut i = 2;
    let mut payload_type: u32 = 0;
    while i < nal.len() && nal[i] == 0xFF {
        payload_type += 255;
        i += 1;
    }
    if i >= nal.len() {
        return false;
    }
    payload_type += nal[i] as u32;
    i += 1;
    if payload_type != 4 {
        // 4 = user_data_registered_itu_t_t35
        return false;
    }
    // Skip the variable-length payload size field.
    while i < nal.len() && nal[i] == 0xFF {
        i += 1;
    }
    i += 1; // skip final size byte
            // Check ITU-T T35 header: country=0xB5, provider=0x003C, oriented=0x0001
    i + 5 <= nal.len()
        && nal[i] == 0xB5
        && nal[i + 1] == 0x00
        && nal[i + 2] == 0x3C
        && nal[i + 3] == 0x00
        && nal[i + 4] == 0x01
}

// Annex-B utilities
fn find_start_code_positions(data: &[u8]) -> Vec<usize> {
    let mut positions = Vec::new();
    let len = data.len();
    let mut i = 0;
    while i + 2 < len {
        if data[i] == 0 && data[i + 1] == 0 {
            if i + 3 < len && data[i + 2] == 0 && data[i + 3] == 1 {
                positions.push(i);
                i += 4;
                continue;
            }
            if data[i + 2] == 1 {
                positions.push(i);
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    positions
}

fn start_code_len(data: &[u8]) -> usize {
    if data.len() >= 4 && data[0] == 0 && data[1] == 0 && data[2] == 0 && data[3] == 1 {
        4
    } else if data.len() >= 3 && data[0] == 0 && data[1] == 0 && data[2] == 1 {
        3
    } else {
        0
    }
}

// MKV EBML RPU rewriter
//
// Parses a streaming Matroska/WebM byte stream, locates BlockGroup elements
// inside Cluster(s), extracts RPU payloads from BlockAdditional (DV EL track),
// converts them via libdovi, injects the converted RPU as an in-band NAL at the
// end of the base-layer Block's frame data, and drops the BlockAdditions element.

// EBML element IDs
const EBML_CLUSTER: u64 = 0x1F43_B675;
const EBML_BLOCK_GROUP: u64 = 0xA0;
const EBML_BLOCK: u64 = 0xA1;
#[allow(dead_code)]
const EBML_SIMPLE_BLOCK: u64 = 0xA3;
const EBML_BLOCK_ADDITIONS: u64 = 0x75A1;
const EBML_BLOCK_MORE: u64 = 0xA6;
const EBML_BLOCK_ADD_ID: u64 = 0xEE;
const EBML_BLOCK_ADDITIONAL: u64 = 0xA5;

/// DV EL track BlockAddID value.
const DV_BLOCK_ADD_ID: u64 = 1;

/// Sentinel value for unknown-size EBML element.
const EBML_UNKNOWN_SIZE: u64 = u64::MAX;

// EBML primitive functions
/// Returns the byte-width of an EBML element ID whose first byte is `first_byte`.
/// EBML IDs use a leading 1-bit to signal width (same as vint but marker bits
/// are part of the ID itself).
mod mkv;

pub(crate) use mkv::*;
#[cfg(test)]
mod tests {
    use super::dvcc::{
        apply_patch_at_offset as apply_dvcc_patch_at_offset, mangle_fourcc as mangle_dvcc_fourcc,
        parse_content_range_start,
    };
    use super::*;

    // DVCC fourcc mangler
    #[test]
    fn mangle_dvcc_rewrites_dvcc_config_box() {
        let mut data = b"xxdvcCxx".to_vec();
        let count = mangle_dvcc_fourcc(&mut data);
        assert_eq!(&data, b"xxXXXXxx");
        assert_eq!(count, 1);
    }

    #[test]
    fn mangle_dvcc_rewrites_dvvc_av1_config_box() {
        let mut data = b"dvvCdata".to_vec();
        let count = mangle_dvcc_fourcc(&mut data);
        assert_eq!(&data[..4], b"XXXX");
        assert_eq!(count, 1);
    }

    #[test]
    fn mangle_dvcc_rewrites_dvhe_sample_entry() {
        let mut data = b"xxdvhexx".to_vec();
        let count = mangle_dvcc_fourcc(&mut data);
        assert_eq!(&data[2..6], b"XXXX");
        assert_eq!(count, 1);
    }

    #[test]
    fn mangle_dvcc_rewrites_dvh1_sample_entry() {
        let mut data = b"xxdvh1xx".to_vec();
        let count = mangle_dvcc_fourcc(&mut data);
        assert_eq!(&data[2..6], b"XXXX");
        assert_eq!(count, 1);
    }

    #[test]
    fn mangle_dvcc_rewrites_all_four_patterns_in_one_pass() {
        let mut data = b"dvcCdvvCdvhedvh1".to_vec();
        let count = mangle_dvcc_fourcc(&mut data);
        assert_eq!(count, 4);
        assert_eq!(data, vec![b'X'; 16]);
    }

    #[test]
    fn mangle_dvcc_does_not_rewrite_lowercase_dvcc() {
        let mut data = b"xxdvccxx".to_vec();
        let original = data.clone();
        let count = mangle_dvcc_fourcc(&mut data);
        assert_eq!(data, original, "lowercase dvcc is not a known DV box type");
        assert_eq!(count, 0);
    }

    #[test]
    fn mangle_dvcc_rewrites_multiple_occurrences() {
        let mut data = b"aadvcCzzdvheqq".to_vec();
        let count = mangle_dvcc_fourcc(&mut data);
        assert!(!data.windows(4).any(|w| w == b"dvcC" || w == b"dvhe"));
        assert_eq!(count, 2);
    }

    #[test]
    fn mangle_dvcc_leaves_unrelated_data_intact() {
        let mut data = b"hevcavchdr10".to_vec();
        let original = data.clone();
        let count = mangle_dvcc_fourcc(&mut data);
        assert_eq!(data, original);
        assert_eq!(count, 0);
    }

    #[test]
    fn mangle_dvcc_handles_boundary_at_end() {
        let mut data = b"12345678dvcC".to_vec();
        let count = mangle_dvcc_fourcc(&mut data);
        assert_eq!(&data[8..], b"XXXX");
        assert_eq!(count, 1);
    }

    // parse_content_range_start
    #[test]
    fn parse_range_full_file_from_zero() {
        assert_eq!(parse_content_range_start("bytes 0-999999/1000000"), Some(0));
    }

    #[test]
    fn parse_range_mid_file_seek() {
        assert_eq!(
            parse_content_range_start("bytes 50000-100000/1000000"),
            Some(50000)
        );
    }

    #[test]
    fn parse_range_past_window() {
        assert_eq!(
            parse_content_range_start("bytes 131072-200000/5000000"),
            Some(131072)
        );
    }

    #[test]
    fn parse_range_invalid_returns_none() {
        assert_eq!(parse_content_range_start("invalid header"), None);
        assert_eq!(parse_content_range_start("bytes */*"), None);
    }

    // apply_dvcc_patch_at_offset (range-aware patching)
    #[test]
    fn patch_at_offset_zero_patches_normally() {
        let mut data = b"xxdvcCxx".to_vec();
        let count = apply_dvcc_patch_at_offset(&mut data, 0, 65536);
        assert_eq!(count, 1);
        assert_eq!(&data[2..6], b"XXXX");
    }

    #[test]
    fn patch_at_offset_past_window_skips_entirely() {
        let mut data = b"xxdvcCxx".to_vec();
        let original = data.clone();
        let count = apply_dvcc_patch_at_offset(&mut data, 65536, 65536);
        assert_eq!(count, 0);
        assert_eq!(data, original, "data past scan window must be untouched");
    }

    #[test]
    fn patch_at_offset_small_range_within_window() {
        let mut data = vec![0u8; 1024];
        data[512..516].copy_from_slice(b"dvcC");
        let count = apply_dvcc_patch_at_offset(&mut data, 0, 65536);
        assert_eq!(count, 1);
        assert_eq!(&data[512..516], b"XXXX");
    }

    #[test]
    fn patch_at_offset_overlapping_range_patches_only_window_portion() {
        let mut data = b"dvcCxxxx".to_vec();
        let count = apply_dvcc_patch_at_offset(&mut data, 65530, 65536);
        assert_eq!(
            count, 1,
            "dvcC at file offset 65530 is inside the scan window"
        );
        assert_eq!(&data[..4], b"XXXX");
    }

    #[test]
    fn patch_at_offset_fourcc_straddles_window_boundary_not_patched() {
        let mut data = b"dvcCxxxx".to_vec();
        let count = apply_dvcc_patch_at_offset(&mut data, 65534, 65536);
        assert_eq!(
            count, 0,
            "partial match at window boundary must not be patched"
        );
        assert_eq!(
            &data[..4],
            b"dvcC",
            "straddling fourcc must remain unchanged"
        );
    }

    #[test]
    fn patch_at_offset_range_100kb_no_patch_needed() {
        let mut data = b"xxdvcCxx".to_vec();
        let original = data.clone();
        let count = apply_dvcc_patch_at_offset(&mut data, 102400, 65536);
        assert_eq!(count, 0);
        assert_eq!(data, original);
    }

    // dvcC box parser
    fn make_dvcc_box(profile: u8, compat_id: u8) -> Vec<u8> {
        // Build a minimal dvcC box: 4-byte size + "dvcC" + 8 bytes payload.
        // byte[2] = (profile << 1) | (level_high_bit)  — level = 0 for tests
        // byte[4] = (compat_id << 4)
        let mut v = Vec::new();
        v.extend_from_slice(&[0x00, 0x00, 0x00, 0x10]); // size = 16
        v.extend_from_slice(b"dvcC");
        v.push(1); // dv_version_major
        v.push(0); // dv_version_minor
        v.push((profile << 1) & 0xFE); // byte[2]: profile in bits [7:1]
        v.push(0x00); // byte[3]: level low bits + flags
        v.push((compat_id << 4) & 0xF0); // byte[4]: compat_id in bits [7:4]
        v.extend_from_slice(&[0x00, 0x00, 0x00]); // reserved
        v
    }

    #[test]
    fn parse_dvcc_reads_profile_and_compat_id() {
        let box_data = make_dvcc_box(7, 6);
        // scan_dvcc_info looks for "dvcC" and reads 5 bytes after it
        let info = scan_dvcc_info(&box_data).expect("should parse dvcC");
        assert_eq!(info.profile, 7);
        assert_eq!(info.compat_id, 6);
    }

    #[test]
    fn parse_dvcc_profile8_no_compat() {
        let box_data = make_dvcc_box(8, 0);
        let info = scan_dvcc_info(&box_data).expect("should parse profile 8");
        assert_eq!(info.profile, 8);
        assert_eq!(info.compat_id, 0);
    }

    #[test]
    fn parse_dvcc_profile10_compat1_has_hdr10_fallback() {
        let box_data = make_dvcc_box(10, 1);
        let info = scan_dvcc_info(&box_data).expect("should parse profile 10 compat 1");
        assert!(!info.not_has_hdr10_fallback(), "compat_id=1 has HDR10 base");
    }

    #[test]
    fn parse_dvcc_profile10_compat0_no_hdr10_fallback() {
        let box_data = make_dvcc_box(10, 0);
        let info = scan_dvcc_info(&box_data).unwrap();
        assert!(info.not_has_hdr10_fallback(), "compat_id=0 is DV-only");
    }

    #[test]
    fn parse_dvcc_profile4_always_no_fallback() {
        let box_data = make_dvcc_box(4, 0);
        let info = scan_dvcc_info(&box_data).unwrap();
        assert!(info.not_has_hdr10_fallback());
    }

    #[test]
    fn parse_dvcc_profile5_cid0_no_fallback() {
        let box_data = make_dvcc_box(5, 0);
        let info = scan_dvcc_info(&box_data).unwrap();
        assert!(info.not_has_hdr10_fallback());
    }

    #[test]
    fn parse_dvcc_profile5_cid1_has_hdr10_fallback() {
        let box_data = make_dvcc_box(5, 1);
        let info = scan_dvcc_info(&box_data).unwrap();
        assert!(
            !info.not_has_hdr10_fallback(),
            "P5 CID=1 has HDR10 base layer"
        );
    }

    #[test]
    fn scan_dvcc_finds_box_in_larger_buffer() {
        let mut buf = vec![0xAA; 128];
        let box_data = make_dvcc_box(7, 6);
        buf[64..64 + box_data.len()].copy_from_slice(&box_data);
        let info = scan_dvcc_info(&buf).expect("should find dvcC at offset 68");
        assert_eq!(info.profile, 7);
    }

    #[test]
    fn scan_dvcc_returns_none_when_absent() {
        let buf = b"hevc hvcC data without any dolby vision boxes".to_vec();
        assert!(scan_dvcc_info(&buf).is_none());
    }

    // HDR10+ SEI detector
    fn make_sei_nal(nal_type: u8, payload_type: u8, payload: &[u8]) -> Vec<u8> {
        // 2-byte HEVC NAL header + 1-byte SEI type + 1-byte SEI size + payload
        let header = [(nal_type << 1) & 0xFE, 0x01u8];
        let mut v = Vec::new();
        v.extend_from_slice(&header);
        v.push(payload_type);
        v.push(payload.len() as u8);
        v.extend_from_slice(payload);
        v
    }

    fn hdr10plus_payload() -> Vec<u8> {
        // Minimal ITU-T T35 HDR10+ payload: country=B5, provider=003C, oriented=0001
        vec![0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04, 0x08]
    }

    #[test]
    fn hdr10plus_sei_detected_in_prefix_sei() {
        let payload = hdr10plus_payload();
        // PREFIX_SEI = nal_type 39, payload_type 4 = user_data_registered_itu_t_t35
        let nal = make_sei_nal(39, 4, &payload);
        assert!(
            nal_is_hdr10plus_sei(&nal),
            "prefix SEI with HDR10+ payload must be detected"
        );
    }

    #[test]
    fn hdr10plus_sei_detected_in_suffix_sei() {
        let payload = hdr10plus_payload();
        let nal = make_sei_nal(40, 4, &payload);
        assert!(nal_is_hdr10plus_sei(&nal));
    }

    #[test]
    fn non_hdr10plus_sei_not_detected() {
        // payload_type 5 = user_data_unregistered — not HDR10+
        let nal = make_sei_nal(39, 5, &[0xDE, 0xAD, 0xBE, 0xEF, 0xFF]);
        assert!(!nal_is_hdr10plus_sei(&nal));
    }

    #[test]
    fn non_sei_nal_not_detected() {
        // NAL type 19 = IDR frame — not an SEI
        let nal = make_sei_nal(19, 4, &hdr10plus_payload());
        assert!(!nal_is_hdr10plus_sei(&nal));
    }

    #[test]
    fn hdr10plus_sei_wrong_provider_not_detected() {
        // T35 with different provider code (e.g. HDR Vivid = 0x0026)
        let payload = vec![0xB5, 0x00, 0x26, 0x00, 0x01, 0x04];
        let nal = make_sei_nal(39, 4, &payload);
        assert!(!nal_is_hdr10plus_sei(&nal));
    }

    // Annex-B start code finder
    #[test]
    fn find_positions_finds_3_byte_start_code() {
        let data = [0x00, 0x00, 0x01, 0x09, 0xFF];
        let positions = find_start_code_positions(&data);
        assert_eq!(positions, vec![0]);
    }

    #[test]
    fn find_positions_finds_4_byte_start_code() {
        let data = [0x00, 0x00, 0x00, 0x01, 0x09, 0xFF];
        let positions = find_start_code_positions(&data);
        assert_eq!(positions, vec![0]);
    }

    #[test]
    fn find_positions_finds_multiple_start_codes() {
        let data = [0x00, 0x00, 0x01, 0x09, 0xFF, 0x00, 0x00, 0x01, 0x67, 0x00];
        let positions = find_start_code_positions(&data);
        assert_eq!(positions, vec![0, 5]);
    }

    #[test]
    fn find_positions_ignores_partial_start_code_at_end() {
        let data = [0x00, 0x00, 0x01, 0x09, 0x00, 0x00];
        let positions = find_start_code_positions(&data);
        assert_eq!(positions, vec![0]);
    }

    #[test]
    fn find_positions_empty_data_returns_empty() {
        assert!(find_start_code_positions(&[]).is_empty());
    }

    // start_code_len
    #[test]
    fn start_code_len_4_byte() {
        assert_eq!(start_code_len(&[0x00, 0x00, 0x00, 0x01, 0x09]), 4);
    }

    #[test]
    fn start_code_len_3_byte() {
        assert_eq!(start_code_len(&[0x00, 0x00, 0x01, 0x09]), 3);
    }

    #[test]
    fn start_code_len_no_match() {
        assert_eq!(start_code_len(&[0x00, 0x01, 0x09]), 0);
    }

    // NAL rewrite state machine
    fn make_nal(nal_type: u8, payload: &[u8]) -> Vec<u8> {
        let header = [(nal_type << 1) & 0xFE, 0x01u8];
        let mut v = vec![0x00, 0x00, 0x00, 0x01];
        v.extend_from_slice(&header);
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn nal_state_passes_through_non_rpu_nals_unchanged() {
        let input = make_nal(35, &[0xAA, 0xBB, 0xCC]);
        let second = make_nal(1, &[0x11]);
        let mut state = NalRewriteState::new(2);
        let partial = state.process(&input);
        assert!(
            partial.is_empty(),
            "single NAL must be buffered until next start code"
        );
        let out = state.process(&second);
        assert!(
            out.windows(input.len()).any(|w| w == input.as_slice()),
            "non-RPU NAL must be emitted unchanged"
        );
    }

    #[test]
    fn nal_state_falls_back_to_original_on_invalid_rpu_data() {
        let rpu_nal = make_nal(62, &[0xDE, 0xAD, 0xBE, 0xEF]);
        let second = make_nal(1, &[0x11]);
        let mut input = rpu_nal.clone();
        input.extend_from_slice(&second);

        let mut state = NalRewriteState::new(2);
        let out = state.process(&input);
        assert!(
            out.windows(rpu_nal.len()).any(|w| w == rpu_nal.as_slice()),
            "failed RPU conversion must leave original NAL intact"
        );
    }

    #[test]
    fn nal_state_flush_emits_last_pending_nal() {
        let nal = make_nal(9, &[0xFF]);
        let mut state = NalRewriteState::new(2);
        let mid = state.process(&nal);
        assert!(mid.is_empty());
        let tail = state.flush();
        assert_eq!(tail, nal, "flush must emit the buffered NAL unchanged");
    }

    #[test]
    fn nal_state_handles_chunk_spanning_nal_boundary() {
        let nal1 = make_nal(9, &[0xAA, 0xBB]);
        let nal2 = make_nal(5, &[0xCC]);
        let combined = [nal1.as_slice(), nal2.as_slice()].concat();

        let split = nal1.len() - 1;
        let mut state = NalRewriteState::new(2);
        let first_out = state.process(&combined[..split]);
        let second_out = state.process(&combined[split..]);
        let flushed = state.flush();
        let all_out = [first_out, second_out, flushed].concat();

        assert_eq!(
            all_out, combined,
            "chunked input must produce identical output"
        );
    }

    #[test]
    fn hdr10plus_strip_state_removes_hdr10plus_sei() {
        let hdr10plus_nal = {
            let payload = vec![0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04, 0x08];
            make_sei_nal(39, 4, &payload)
        };
        // Add start codes around it so the state machine can delimit it.
        let sc_nal = {
            let mut v = vec![0x00, 0x00, 0x00, 0x01];
            v.extend_from_slice(&hdr10plus_nal);
            v
        };
        let next_nal = make_nal(1, &[0x11]);
        let mut input = sc_nal.clone();
        input.extend_from_slice(&next_nal);

        let mut state = NalRewriteState::new_hdr10plus_strip();
        let out = state.process(&input);
        let flushed = state.flush();
        let all_out = [out, flushed].concat();

        // The HDR10+ SEI must not appear in the output.
        assert!(
            !all_out
                .windows(hdr10plus_nal.len())
                .any(|w| w == hdr10plus_nal.as_slice()),
            "HDR10+ SEI NAL must be stripped"
        );
        // The subsequent non-HDR10+ NAL must survive.
        let nal_data = &next_nal[4..]; // skip start code
        assert!(
            all_out.windows(nal_data.len()).any(|w| w == nal_data),
            "non-HDR10+ NAL must be kept"
        );
    }

    #[test]
    fn hdr10plus_strip_state_keeps_non_hdr10plus_nals() {
        let regular_sei = make_nal(39, &[0x05, 0x04, 0xDE, 0xAD, 0xBE, 0xEF]); // unregistered
        let next_nal = make_nal(1, &[0x22]);
        let mut input = regular_sei.clone();
        input.extend_from_slice(&next_nal);

        let mut state = NalRewriteState::new_hdr10plus_strip();
        let out = state.process(&input);
        let flushed = state.flush();
        let all_out = [out, flushed].concat();

        let nal_data = &regular_sei[4..];
        assert!(
            all_out.windows(nal_data.len()).any(|w| w == nal_data),
            "non-HDR10+ SEI must be kept"
        );
    }

    // length-delimited NAL rewriter
    /// Build a 4-byte-length-prefixed HEVC NAL unit for fMP4 testing.
    fn make_ld_nal(nal_type: u8, layer_id: u8, payload: &[u8]) -> Vec<u8> {
        // HEVC NAL header:
        //   byte[0] = (nal_type << 1) & 0xFE | (layer_id >> 5)
        //   byte[1] = ((layer_id & 0x1F) << 3) | temporal_id_plus1
        let byte0 = ((nal_type << 1) & 0xFE) | ((layer_id >> 5) & 0x01);
        let byte1 = ((layer_id & 0x1F) << 3) | 0x01; // temporal_id_plus1 = 1
        let nal_len = 2 + payload.len();
        let mut v = (nal_len as u32).to_be_bytes().to_vec();
        v.push(byte0);
        v.push(byte1);
        v.extend_from_slice(payload);
        v
    }

    /// Wrap a raw mdat payload in a minimal ISO-BMFF mdat box.
    fn make_mdat_box(content: &[u8]) -> Vec<u8> {
        let box_size = (content.len() + 8) as u32;
        let mut v = box_size.to_be_bytes().to_vec();
        v.extend_from_slice(b"mdat");
        v.extend_from_slice(content);
        v
    }

    #[test]
    fn ld_rewriter_passes_bl_nal_unchanged() {
        let bl = make_ld_nal(1, 0, &[0xAA, 0xBB]);
        let (out, rpu, _, el) = rewrite_length_delimited_nals(&bl, 2, false, false);
        assert_eq!(out, bl, "BL NAL must pass through unchanged");
        assert_eq!(rpu, 0);
        assert_eq!(el, 0);
    }

    #[test]
    fn ld_rewriter_drops_el_nal() {
        let el = make_ld_nal(1, 1, &[0xCC, 0xDD]); // layer_id=1 → EL
        let (out, rpu, _, el_count) = rewrite_length_delimited_nals(&el, 2, false, false);
        assert!(out.is_empty(), "EL NAL must be dropped");
        assert_eq!(el_count, 1);
        assert_eq!(rpu, 0);
    }

    #[test]
    fn ld_rewriter_keeps_invalid_rpu_unchanged() {
        // libdovi will reject this garbage payload — must fall back to original.
        let rpu_nal = make_ld_nal(62, 0, &[0xDE, 0xAD, 0xBE, 0xEF]);
        let (out, rpu_count, _, _) = rewrite_length_delimited_nals(&rpu_nal, 2, false, false);
        assert_eq!(out, rpu_nal, "failed RPU conversion must keep original NAL");
        assert_eq!(rpu_count, 0, "failed conversion must not increment counter");
    }

    #[test]
    fn ld_rewriter_mixed_sample_keeps_bl_drops_el() {
        let bl_payload = vec![0xAA, 0xAA, 0xAA, 0xAA];
        let el_payload = vec![0xBB, 0xBB, 0xBB, 0xBB];
        let other_payload = vec![0xCC, 0xCC, 0xCC, 0xCC];
        let mut mdat = Vec::new();
        mdat.extend_from_slice(&make_ld_nal(19, 0, &bl_payload)); // IDR BL
        mdat.extend_from_slice(&make_ld_nal(1, 1, &el_payload)); // EL → drop
        mdat.extend_from_slice(&make_ld_nal(35, 0, &other_payload)); // AUD BL

        let (out, _, _, el_count) = rewrite_length_delimited_nals(&mdat, 2, false, false);
        assert_eq!(el_count, 1, "exactly one EL NAL must be dropped");
        assert!(
            out.windows(4).any(|w| w == bl_payload.as_slice()),
            "BL IDR payload must be in output"
        );
        assert!(
            !out.windows(4).any(|w| w == el_payload.as_slice()),
            "EL payload must not be in output"
        );
        assert!(
            out.windows(4).any(|w| w == other_payload.as_slice()),
            "other BL NAL payload must be in output"
        );
    }

    // FMp4NalRewriter state machine
    #[test]
    fn fmp4_rewriter_forwards_non_mdat_box_unchanged() {
        // ftyp box: size=16, type="ftyp", 8 bytes of content
        let mut ftyp = (16u32).to_be_bytes().to_vec();
        ftyp.extend_from_slice(b"ftyp");
        ftyp.extend_from_slice(b"iso5");
        ftyp.extend_from_slice(&[0u8; 4]);

        let mut rewriter = FMp4NalRewriter::new(2, false, false);
        let out = rewriter.process(&ftyp);
        let flushed = rewriter.flush();
        let all = [out, flushed].concat();

        assert_eq!(all, ftyp, "non-mdat box must be forwarded byte-for-byte");
    }

    #[test]
    fn fmp4_rewriter_processes_mdat_and_updates_box_size() {
        let bl_payload = vec![0x11u8; 8];
        let el_payload = vec![0x22u8; 8];
        let mut mdat_content = Vec::new();
        mdat_content.extend_from_slice(&make_ld_nal(19, 0, &bl_payload)); // BL
        mdat_content.extend_from_slice(&make_ld_nal(1, 1, &el_payload)); // EL → drop

        let segment = make_mdat_box(&mdat_content);

        let mut rewriter = FMp4NalRewriter::new(2, false, false);
        let out = rewriter.process(&segment);
        let flushed = rewriter.flush();
        let all = [out, flushed].concat();

        // mdat fourcc must be present
        assert!(
            all.windows(4).any(|w| w == b"mdat"),
            "mdat fourcc must be in output"
        );
        // box size in output must equal actual content size + 8
        let out_box_size = u32::from_be_bytes([all[0], all[1], all[2], all[3]]) as usize;
        assert_eq!(
            out_box_size,
            all.len(),
            "mdat box size must match actual output length"
        );
        // BL payload must be present, EL must be absent
        assert!(all.windows(8).any(|w| w == bl_payload.as_slice()));
        assert!(!all.windows(8).any(|w| w == el_payload.as_slice()));
    }

    #[test]
    fn fmp4_rewriter_handles_moof_plus_mdat() {
        // Minimal moof box (header only, 8 bytes, no content)
        let mut moof = (8u32).to_be_bytes().to_vec();
        moof.extend_from_slice(b"moof");

        let bl_payload = vec![0x55u8, 0x66, 0x77, 0x88];
        let mdat = make_mdat_box(&make_ld_nal(1, 0, &bl_payload));
        let segment = [moof.clone(), mdat].concat();

        let mut rewriter = FMp4NalRewriter::new(2, false, false);
        let out = rewriter.process(&segment);
        let flushed = rewriter.flush();
        let all = [out, flushed].concat();

        assert!(
            all.windows(4).any(|w| w == b"moof"),
            "moof must be forwarded"
        );
        assert!(all.windows(4).any(|w| w == b"mdat"), "mdat must be present");
        assert!(
            all.windows(4).any(|w| w == bl_payload.as_slice()),
            "BL payload must survive"
        );
    }

    #[test]
    fn fmp4_rewriter_handles_mdat_split_across_chunks() {
        let bl_payload = vec![0xAAu8, 0xBB, 0xCC, 0xDD];
        let segment = make_mdat_box(&make_ld_nal(5, 0, &bl_payload));

        // Split at byte 6 — right in the middle of the mdat header
        let (first, second) = segment.split_at(6);

        let mut rewriter = FMp4NalRewriter::new(2, false, false);
        let out1 = rewriter.process(first);
        let out2 = rewriter.process(second);
        let flushed = rewriter.flush();
        let all = [out1, out2, flushed].concat();

        assert!(all.windows(4).any(|w| w == b"mdat"));
        assert!(all.windows(4).any(|w| w == bl_payload.as_slice()));
    }

    #[test]
    fn fmp4_rewriter_handles_empty_mdat() {
        // mdat with 0 bytes of content (size=8, just the header)
        let mut empty_mdat = (8u32).to_be_bytes().to_vec();
        empty_mdat.extend_from_slice(b"mdat");

        let mut rewriter = FMp4NalRewriter::new(2, false, false);
        let out = rewriter.process(&empty_mdat);
        let flushed = rewriter.flush();
        let all = [out, flushed].concat();

        assert_eq!(all, empty_mdat, "empty mdat must be forwarded unchanged");
    }

    #[test]
    fn fmp4_rewriter_processes_multiple_mdat_boxes() {
        let payload_a = vec![0x1Au8; 4];
        let payload_b = vec![0x2Bu8; 4];
        let seg_a = make_mdat_box(&make_ld_nal(1, 0, &payload_a));
        let seg_b = make_mdat_box(&make_ld_nal(1, 0, &payload_b));
        let combined = [seg_a, seg_b].concat();

        let mut rewriter = FMp4NalRewriter::new(2, false, false);
        let out = rewriter.process(&combined);
        let flushed = rewriter.flush();
        let all = [out, flushed].concat();

        assert!(all.windows(4).any(|w| w == payload_a.as_slice()));
        assert!(all.windows(4).any(|w| w == payload_b.as_slice()));
    }

    // EBML primitive tests
    #[test]
    fn ebml_id_width_correct() {
        assert_eq!(ebml_id_width(0xA0), 1); // BlockGroup 0xA0
        assert_eq!(ebml_id_width(0x40), 2); // 2-byte IDs
        assert_eq!(ebml_id_width(0x20), 3);
        assert_eq!(ebml_id_width(0x1F), 4); // Cluster 0x1F43B675 starts with 0x1F
        assert_eq!(ebml_id_width(0x00), 0); // invalid
    }

    #[test]
    fn parse_ebml_id_1_byte() {
        // BlockGroup = 0xA0 (single byte ID)
        let buf = [0xA0u8, 0x83, 0x01, 0x02, 0x03];
        let (id, consumed) = parse_ebml_id(&buf).unwrap();
        assert_eq!(id, 0xA0u64);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn parse_ebml_id_4_byte() {
        // Cluster = 0x1F43B675
        let buf = [
            0x1Fu8, 0x43, 0xB6, 0x75, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        ];
        let (id, consumed) = parse_ebml_id(&buf).unwrap();
        assert_eq!(id, 0x1F43_B675u64);
        assert_eq!(consumed, 4);
    }

    #[test]
    fn parse_ebml_vint_known_size() {
        // 0x83 = binary 1000_0011 → width 1, value = 0x83 & ~0x80 = 0x03 = 3
        let buf = [0x83u8];
        let (val, consumed) = parse_ebml_vint(&buf).unwrap();
        assert_eq!(val, 3);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn parse_ebml_vint_unknown_size() {
        // 0xFF = 1111_1111 → width 1, all data bits set → unknown size
        let buf = [0xFFu8];
        let (val, consumed) = parse_ebml_vint(&buf).unwrap();
        assert_eq!(val, EBML_UNKNOWN_SIZE);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn encode_decode_vint_roundtrip() {
        for &value in &[
            0u64,
            1,
            42,
            126,
            127,
            128,
            16383,
            16384,
            2_000_000,
            268_435_455,
        ] {
            let encoded = encode_ebml_vint(value);
            let (decoded, _) = parse_ebml_vint(&encoded).unwrap();
            assert_eq!(decoded, value, "roundtrip failed for value {value}");
        }
    }

    // BlockGroup processor helpers
    /// Build a minimal EBML element: ID + vint size + data.
    fn make_ebml_elem(id: u64, data: &[u8]) -> Vec<u8> {
        encode_ebml_element(id, data)
    }

    /// Build a minimal Block payload: track VINT (1 byte) + timecode (2 bytes) +
    /// flags (1 byte) + frame data.
    fn make_block_payload(frame: &[u8]) -> Vec<u8> {
        let mut v = vec![
            0x81u8, // track number VINT = 1
            0x00, 0x00, // timecode
            0x00, // flags (no lacing)
        ];
        v.extend_from_slice(frame);
        v
    }

    #[test]
    fn block_group_passthrough_when_no_rpu() {
        // BlockGroup with only a Block element (no BlockAdditions) → unchanged.
        let frame = vec![0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05]; // Annex-B frame
        let block_payload = make_block_payload(&frame);
        let block_elem = make_ebml_elem(EBML_BLOCK, &block_payload);
        let bg_data = block_elem.clone();

        let (result, count) = process_block_group_data(&bg_data, 2, false);
        assert_eq!(count, 0, "no RPU should be injected");
        // When no RPU is found, the original data is returned unchanged.
        assert_eq!(result, bg_data);
    }

    #[test]
    fn block_group_rpu_injection_strips_block_additions() {
        // BlockGroup with Block + BlockAdditions(BlockMore(BlockAddID=1, BlockAdditional=bad_rpu)).
        // Bad RPU → convert fails → Block unchanged, BlockAdditions stripped.
        let frame = vec![0x00, 0x00, 0x00, 0x01, 0x11, 0x22, 0x33];
        let block_payload = make_block_payload(&frame);
        let block_elem = make_ebml_elem(EBML_BLOCK, &block_payload);

        // BlockAdditional: garbage RPU data (will fail conversion).
        let bad_rpu = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let block_additional = make_ebml_elem(EBML_BLOCK_ADDITIONAL, &bad_rpu);
        let block_add_id = make_ebml_elem(EBML_BLOCK_ADD_ID, &[0x01]); // id=1 as 1 byte integer
        let mut block_more_data = Vec::new();
        block_more_data.extend_from_slice(&block_add_id);
        block_more_data.extend_from_slice(&block_additional);
        let block_more = make_ebml_elem(EBML_BLOCK_MORE, &block_more_data);
        let block_additions = make_ebml_elem(EBML_BLOCK_ADDITIONS, &block_more);

        let mut bg_data = Vec::new();
        bg_data.extend_from_slice(&block_elem);
        bg_data.extend_from_slice(&block_additions);

        let (result, count) = process_block_group_data(&bg_data, 2, false);
        // RPU was found (add_id=1) but conversion failed → count=0.
        assert_eq!(count, 0, "bad RPU should not be counted as injected");
        // BlockAdditions must be stripped from output.
        let has_block_additions = result.windows(2).any(|w| {
            // EBML_BLOCK_ADDITIONS = 0x75A1 → 2 bytes
            w[0] == 0x75 && w[1] == 0xA1
        });
        assert!(
            !has_block_additions,
            "BlockAdditions must be stripped even when RPU conversion fails"
        );
        // Block content must still be present.
        assert!(
            result.windows(frame.len()).any(|w| w == frame.as_slice()),
            "Block frame data must be preserved"
        );
    }

    #[test]
    fn inject_rpu_annexb_framing() {
        // Block with Annex-B 4-byte start code frame data → RPU appended with start code.
        let frame = [0x00, 0x00, 0x00, 0x01, 0x01, 0x02, 0x03];
        let block = make_block_payload(&frame);
        let rpu = [0xAA, 0xBB, 0xCC];
        let result = inject_rpu_into_mkv_block(&block, &rpu);
        // Output should end with: 0x00 0x00 0x00 0x01 + rpu
        let expected_suffix = [0x00, 0x00, 0x00, 0x01, 0xAA, 0xBB, 0xCC];
        assert!(
            result.ends_with(&expected_suffix),
            "Annex-B RPU must be appended with 4-byte start code"
        );
    }

    #[test]
    fn inject_rpu_length_delimited_framing() {
        // Block with length-delimited frame data (4-byte size prefix) → RPU appended with BE size.
        let nal_payload = [0x01, 0x02, 0x03, 0x04];
        let mut frame = (nal_payload.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&nal_payload);
        let block = make_block_payload(&frame);
        let rpu = [0xDD, 0xEE, 0xFF];
        let result = inject_rpu_into_mkv_block(&block, &rpu);
        // Should end with: big-endian length (3) + rpu
        let expected_suffix = [0x00, 0x00, 0x00, 0x03, 0xDD, 0xEE, 0xFF];
        assert!(
            result.ends_with(&expected_suffix),
            "Length-delimited RPU must be appended with 4-byte BE size prefix"
        );
    }

    // MkvRpuRewriter integration test
    /// Build a minimal EBML header (the file-level EBML element).
    fn make_ebml_header() -> Vec<u8> {
        // EBML element (0x1A45DFA3) with minimal content.
        let content = make_ebml_elem(0x4286u64, &[0x01]); // EBMLVersion = 1
                                                          // EBML ID = 0x1A45DFA3 (4 bytes) + vint size + content.
        let id_bytes = [0x1Au8, 0x45, 0xDF, 0xA3];
        let size_bytes = encode_ebml_vint(content.len() as u64);
        let mut v = Vec::new();
        v.extend_from_slice(&id_bytes);
        v.extend_from_slice(&size_bytes);
        v.extend_from_slice(&content);
        v
    }

    /// Build a Cluster wrapping a single BlockGroup(Block + BlockAdditions).
    fn make_cluster_with_block_group() -> Vec<u8> {
        let frame = vec![0x00, 0x00, 0x00, 0x01, 0x26, 0x01, 0x00, 0x00]; // Annex-B frame
        let block_payload = make_block_payload(&frame);
        let block_elem = make_ebml_elem(EBML_BLOCK, &block_payload);

        // Add a fake RPU BlockAdditions (bad RPU — conversion will fail but
        // we still verify BlockAdditions is stripped from output).
        let bad_rpu = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let block_additional = make_ebml_elem(EBML_BLOCK_ADDITIONAL, &bad_rpu);
        let block_add_id = make_ebml_elem(EBML_BLOCK_ADD_ID, &[0x01]);
        let mut block_more_data = Vec::new();
        block_more_data.extend_from_slice(&block_add_id);
        block_more_data.extend_from_slice(&block_additional);
        let block_more = make_ebml_elem(EBML_BLOCK_MORE, &block_more_data);
        let block_additions = make_ebml_elem(EBML_BLOCK_ADDITIONS, &block_more);

        let mut bg_data = Vec::new();
        bg_data.extend_from_slice(&block_elem);
        bg_data.extend_from_slice(&block_additions);
        let block_group = make_ebml_elem(EBML_BLOCK_GROUP, &bg_data);

        // Cluster = 0x1F43B675.
        let id_bytes = [0x1Fu8, 0x43, 0xB6, 0x75];
        let size_bytes = encode_ebml_vint(block_group.len() as u64);
        let mut cluster = Vec::new();
        cluster.extend_from_slice(&id_bytes);
        cluster.extend_from_slice(&size_bytes);
        cluster.extend_from_slice(&block_group);
        cluster
    }

    #[test]
    fn mkv_rewriter_strips_block_additions_in_one_chunk() {
        let mut data = make_ebml_header();
        data.extend_from_slice(&make_cluster_with_block_group());

        let mut rewriter = MkvRpuRewriter::new(2, false);
        let out1 = rewriter.process(&data);
        let flushed = rewriter.flush();
        let all = [out1, flushed].concat();

        // BlockAdditions (0x75A1) must NOT appear in output.
        let has_block_additions = all.windows(2).any(|w| w[0] == 0x75 && w[1] == 0xA1);
        assert!(
            !has_block_additions,
            "BlockAdditions must be stripped from MKV output"
        );

        // BlockGroup (0xA0) must still be present.
        assert!(
            all.iter().any(|&b| b == 0xA0),
            "BlockGroup must be present in output"
        );
    }

    #[test]
    fn mkv_rewriter_strips_block_additions_split_chunks() {
        let mut data = make_ebml_header();
        data.extend_from_slice(&make_cluster_with_block_group());

        // Split in the middle of the cluster payload.
        let split = data.len() / 2;
        let (first, second) = data.split_at(split);

        let mut rewriter = MkvRpuRewriter::new(2, false);
        let out1 = rewriter.process(first);
        let out2 = rewriter.process(second);
        let flushed = rewriter.flush();
        let all = [out1, out2, flushed].concat();

        let has_block_additions = all.windows(2).any(|w| w[0] == 0x75 && w[1] == 0xA1);
        assert!(
            !has_block_additions,
            "BlockAdditions must be stripped even when split across chunks"
        );
    }
}
