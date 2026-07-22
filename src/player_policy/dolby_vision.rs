use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DvFallbackMode {
    Off,
    #[default]
    Auto,
    Dv8,
    ConvertDv81,
    Hdr10,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DvProxyPlanRequest {
    #[serde(default)]
    stream: Value,
    #[serde(default)]
    url: String,
    #[serde(default)]
    fallback_mode: DvFallbackMode,
    #[serde(default)]
    device_has_dv_decoder: bool,
    #[serde(default)]
    device_has_dv_display: bool,
}

/// Per-stream Dolby Vision profile classification.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DvProfile {
    /// Profile 4 — AVC single-layer, no HDR base layer.
    P4,
    /// Profile 5 — HEVC single-layer, no HDR base layer.
    P5,
    /// Profile 7 — dual-layer BL+EL with HDR10-compatible base.
    P7,
    /// Profile 8, compat_id=1 — single-layer HEVC, HDR10 base. Safest fallback.
    P8Hdr10,
    /// Profile 8, compat_id=4 — single-layer HEVC, HLG base.
    P8Hlg,
    /// Profile 8 with unrecognised compat_id — HDR10 assumed but uncertain.
    P8Unknown,
    /// Profile 10, compat_id=1 — HDR10-compatible base.
    P10Hdr10,
    /// Profile 10, compat_id 0/2/3 — no HDR10 base.
    P10Other,
    /// Profile could not be determined from stream metadata.
    Unknown,
}

impl DvProfile {
    fn label(self) -> &'static str {
        match self {
            DvProfile::P4 => "P4",
            DvProfile::P5 => "P5",
            DvProfile::P7 => "P7",
            DvProfile::P8Hdr10 => "P8.1",
            DvProfile::P8Hlg => "P8.4",
            DvProfile::P8Unknown => "P8",
            DvProfile::P10Hdr10 => "P10_compat1",
            DvProfile::P10Other => "P10_other",
            DvProfile::Unknown => "unknown",
        }
    }
}

/// Returns the recommended DV proxy action for a single stream+URL combination.
///
/// Response fields:
///   action        — "none" | "dvcc_strip" | "rpu_convert"
///   rpuMode       — libdovi convert mode (2 = Profile 8)
///   reason        — machine-readable decision code
///   profile       — detected DV profile ("P7", "P8.1", …, "unknown")
///   compatibility — expected output format ("HDR10", "HLG", "DV8", "DV", "none")
///   safety        — "high" | "medium" | "low" | "none"
///   limitations   — list of known caveats for this action
pub(crate) fn dv_proxy_plan_json(request_json: &str) -> Option<String> {
    let req = serde_json::from_str::<DvProxyPlanRequest>(request_json)
        .map_err(|e| CoreError::BadInput {
            context: "dv_proxy_plan_json",
            detail: e.to_string(),
        })
        .log_discard()?;

    if req.fallback_mode == DvFallbackMode::Off {
        return plan_rich("none", "user_disabled", "unknown", "none", "high", &[]);
    }

    let url_lower = req.url.to_lowercase();
    let is_hls = url_lower.ends_with(".m3u8") || url_lower.contains(".m3u8?");
    let is_dash = url_lower.ends_with(".mpd") || url_lower.contains(".mpd?");

    if !is_dolby_vision_stream(&req.stream, &req.url) {
        return plan_rich("none", "not_dv", "unknown", "none", "high", &[]);
    }

    let native_passthrough = req.device_has_dv_decoder
        && (req.device_has_dv_display || req.fallback_mode != DvFallbackMode::ConvertDv81);
    if native_passthrough {
        return plan_rich("none", "hw_dv_decoder", "unknown", "DV", "high", &[]);
    }

    let profile = detect_dv_profile(&req.stream);
    let container = detect_container(&req.url);

    if is_hls || is_dash {
        if is_hls
            && matches!(profile, DvProfile::P7)
            && req.fallback_mode == DvFallbackMode::ConvertDv81
            && req.device_has_dv_decoder
        {
            return plan_rich(
                "hls_rpu_convert",
                "p7_hls_segment_rpu_convert",
                profile.label(),
                "DV8",
                "medium",
                &[],
            );
        }
        return plan_rich(
            "none",
            "manifest_handled",
            profile.label(),
            "none",
            "high",
            &[],
        );
    }

    // Hard safety gates: profiles with no HDR base layer cannot be safely
    // rewritten — stripping DVCC would expose a DV-only bitstream to an
    // HDR10 decoder, producing corrupted colour.
    match profile {
        DvProfile::P4 | DvProfile::P5 => {
            return plan_rich(
                "none",
                "no_hdr_base_layer",
                profile.label(),
                "none",
                "none",
                &["p4_p5_no_hdr_fallback_possible"],
            );
        }
        DvProfile::P10Other => {
            return plan_rich(
                "none",
                "p10_compat_id_no_hdr_base",
                profile.label(),
                "none",
                "none",
                &["only_p10_compat_id_1_has_hdr10_base"],
            );
        }
        // Unknown profile: do nothing rather than guess and corrupt playback.
        DvProfile::Unknown => {
            return plan_rich(
                "none",
                "unknown_profile_no_safe_fallback",
                "unknown",
                "none",
                "none",
                &["set_dvProfile_field_or_codec_string_for_safe_rewrite"],
            );
        }
        _ => {}
    }

    let mode = req.fallback_mode;

    let (action, compat, safety, reason, limitations): (&str, &str, &str, &str, Vec<&str>) =
        match profile {
            DvProfile::P7 => match (mode, &container) {
                // convert_dv81 + DV decoder (no display): RPU conversion for Annex-B + fMP4.
                // Without a DV decoder, conversion would produce DV8.1 that nothing can decode;
                // fall through to dvcc_strip (handled by the catch-all arm below).
                (DvFallbackMode::ConvertDv81, _) if req.device_has_dv_decoder => (
                    "rpu_convert",
                    "DV8",
                    "medium",
                    "p7_rpu_convert_to_dv81",
                    vec![],
                ),
                (DvFallbackMode::Dv8, DvContainer::RawHevc) => (
                    "rpu_convert",
                    "DV8",
                    "medium",
                    "p7_rpu_convert_to_dv8_annexb",
                    vec!["annexb_only"],
                ),
                // Auto mode + DV-capable display: keep DV via RPU conversion.
                (DvFallbackMode::Auto, DvContainer::RawHevc) if req.device_has_dv_display => (
                    "rpu_convert",
                    "DV8",
                    "medium",
                    "p7_rpu_convert_auto_dv_display_annexb",
                    vec!["annexb_only"],
                ),
                // dv8 mode requested but container is not Annex-B.
                (DvFallbackMode::Dv8, _) => (
                    "dvcc_strip",
                    "HDR10",
                    "medium",
                    "rpu_convert_rejected_not_annexb",
                    vec![
                        "rpu_convert_requires_annexb_hevc",
                        "container_is_not_raw_hevc_fallback_to_dvcc_strip",
                        "header_only_patch",
                        "does_not_transcode",
                        "does_not_remove_rpu_nals",
                    ],
                ),
                _ => (
                    "dvcc_strip",
                    "HDR10",
                    "medium",
                    "p7_dvcc_strip_hdr10_base",
                    vec![
                        "does_not_convert_bitstream",
                        "rpu_nals_remain_in_stream_ignored",
                        "header_only_patch",
                        "does_not_transcode",
                        "does_not_remove_rpu_nals",
                    ],
                ),
            },
            DvProfile::P8Hdr10 => (
                "dvcc_strip",
                "HDR10",
                "low",
                "p8_1_hdr10_compat_base",
                vec![
                    "single_layer_hdr10_base_fully_compatible",
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
            DvProfile::P8Hlg => (
                "dvcc_strip",
                "HLG",
                "medium",
                "p8_4_hlg_compat_base",
                vec![
                    "hlg_base_not_hdr10_color_rendering_may_differ",
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
            DvProfile::P8Unknown => (
                "dvcc_strip",
                "HDR10_assumed",
                "medium",
                "p8_compat_id_unknown_hdr10_assumed",
                vec![
                    "compat_id_unknown_hdr10_base_assumed",
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
            DvProfile::P10Hdr10 => (
                "dvcc_strip",
                "HDR10",
                "medium",
                "p10_compat_id_1_hdr10_base",
                vec![
                    "does_not_convert_bitstream",
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
            _ => (
                "dvcc_strip",
                "HDR10_assumed",
                "medium",
                "unknown_profile_dvcc_strip_fallback",
                vec![
                    "header_only_patch",
                    "does_not_transcode",
                    "does_not_remove_rpu_nals",
                ],
            ),
        };

    plan_rich(
        action,
        reason,
        profile.label(),
        compat,
        safety,
        &limitations,
    )
}

fn plan_rich(
    action: &str,
    reason: &str,
    profile: &str,
    compatibility: &str,
    safety: &str,
    limitations: &[&str],
) -> Option<String> {
    serde_json::to_string(&json!({
        "action": action,
        "rpuMode": 2u8,
        "reason": reason,
        "profile": profile,
        "compatibility": compatibility,
        "safety": safety,
        "limitations": limitations,
    }))
    .ok()
}

/// Derive the DV profile from stream metadata, codec strings, and text hints.
fn detect_dv_profile(stream: &Value) -> DvProfile {
    // 1. Explicit integer fields set by the addon.
    let profile_num = stream
        .get("dvProfile")
        .or_else(|| stream.get("dv_profile"))
        .and_then(Value::as_i64);
    let compat_id = stream
        .get("dvCompatId")
        .or_else(|| stream.get("dvCompatibility"))
        .and_then(Value::as_i64);
    if let Some(p) = profile_num {
        return profile_from_nums(p, compat_id);
    }

    // 2. ISO-BMFF / HLS codec string: "dvhe.07.06", "dvh1.08.01", …
    let codecs = stream.get("codecs").and_then(Value::as_str).unwrap_or("");
    if let Some(p) = parse_dv_codec_string(codecs) {
        return p;
    }

    // 3. Codec token embedded in freetext fields (e.g., "dvhe.07.06 BDRemux").
    let name = stream.get("name").and_then(Value::as_str).unwrap_or("");
    let desc = stream
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let filename = stream
        .get("effectiveFilename")
        .or_else(|| stream.get("filename"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let text = format!("{} {} {}", name, desc, filename);
    if let Some(p) = parse_dv_codec_string(&text) {
        return p;
    }

    // 4. Short profile tokens: "P8.1", "P7", "P8", …
    parse_dv_profile_text(&text).unwrap_or(DvProfile::Unknown)
}

fn profile_from_nums(profile: i64, compat_id: Option<i64>) -> DvProfile {
    match profile {
        4 => DvProfile::P4,
        5 => DvProfile::P5,
        7 => DvProfile::P7,
        8 => match compat_id {
            Some(1) => DvProfile::P8Hdr10,
            Some(4) => DvProfile::P8Hlg,
            _ => DvProfile::P8Unknown,
        },
        10 => match compat_id {
            Some(1) => DvProfile::P10Hdr10,
            _ => DvProfile::P10Other,
        },
        _ => DvProfile::Unknown,
    }
}

/// Parse a DV fourcc codec string such as "dvhe.07.06" → P7.
fn parse_dv_codec_string(text: &str) -> Option<DvProfile> {
    let lower = text.to_lowercase();
    for prefix in &["dvhe.", "dvh1.", "dva1.", "dvav."] {
        if let Some(pos) = lower.find(prefix) {
            let after = &text[pos + prefix.len()..];
            let mut parts = after.splitn(3, '.');
            // Take only the leading digits from each field (e.g. "08" from "08.01 Remux").
            let profile: i64 = leading_digits(parts.next()?)?.parse().ok()?;
            let compat: Option<i64> = parts
                .next()
                .and_then(leading_digits)
                .and_then(|s| s.parse().ok());
            return Some(profile_from_nums(profile, compat));
        }
    }
    None
}

fn leading_digits(s: &str) -> Option<&str> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some(&s[..end])
    }
}

/// Recognise short profile tokens ("P8.1", "P7", "P8") in freetext.
fn parse_dv_profile_text(text: &str) -> Option<DvProfile> {
    // Ordered so longer patterns match before their shorter prefixes.
    let patterns: &[(&str, DvProfile)] = &[
        ("P8.1", DvProfile::P8Hdr10),
        ("P8.4", DvProfile::P8Hlg),
        ("P7", DvProfile::P7),
        ("P8", DvProfile::P8Unknown),
        ("P10", DvProfile::P10Other),
        ("P5", DvProfile::P5),
        ("P4", DvProfile::P4),
    ];
    for (pat, profile) in patterns {
        if contains_word(text, pat) {
            return Some(*profile);
        }
    }
    None
}

/// True when `word` appears in `text` surrounded by non-alphanumeric (or absent) bytes.
fn contains_word(text: &str, word: &str) -> bool {
    let tb = text.as_bytes();
    let wb = word.as_bytes();
    let wlen = wb.len();
    if tb.len() < wlen {
        return false;
    }
    for i in 0..=(tb.len() - wlen) {
        if &tb[i..i + wlen] == wb {
            let before_ok = i == 0 || !tb[i - 1].is_ascii_alphanumeric();
            let after_ok = i + wlen >= tb.len() || !tb[i + wlen].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

/// Returns true when the stream or URL is identifiable as Dolby Vision content.
fn is_dolby_vision_stream(stream: &Value, url: &str) -> bool {
    if stream.get("dv").and_then(Value::as_bool).unwrap_or(false)
        || stream
            .get("dolbyVision")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || stream.get("dvProfile").and_then(Value::as_i64).is_some()
    {
        return true;
    }

    let name = stream.get("name").and_then(Value::as_str).unwrap_or("");
    let desc = stream
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let filename = stream
        .get("effectiveFilename")
        .or_else(|| stream.get("filename"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let lower = format!("{} {} {} {}", name, desc, filename, url).to_lowercase();

    if lower.contains("dvhe")
        || lower.contains("dvh1")
        || lower.contains("dva1")
        || lower.contains("dvav")
        || lower.contains("dolby vision")
        || lower.contains("dolby-vision")
        || lower.contains("dovi")
    {
        return true;
    }

    // "DV" as a standalone token (case-sensitive — avoids "DVD", "HDVD", etc.).
    is_standalone_dv_token(&format!("{} {} {} {}", name, desc, filename, url))
}

fn is_standalone_dv_token(text: &str) -> bool {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 1 < len {
        if bytes[i] == b'D' && bytes[i + 1] == b'V' {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphabetic();
            let after_ok = i + 2 >= len || !bytes[i + 2].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

enum DvContainer {
    Mkv,
    Mp4,
    RawHevc,
    Unknown,
}

fn detect_container(url: &str) -> DvContainer {
    let path = url.split('?').next().unwrap_or(url).to_lowercase();
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "mkv" | "mk3d" | "mka" | "mks" | "webm" => DvContainer::Mkv,
        "mp4" | "m4v" | "m4a" | "mov" => DvContainer::Mp4,
        "hevc" | "h265" | "265" => DvContainer::RawHevc,
        _ => DvContainer::Unknown,
    }
}

pub(crate) fn episode_path_matches_id(path: &str, video_id: &str) -> bool {
    let parts: Vec<&str> = video_id.split(':').collect();
    if parts.len() < 3 {
        return false;
    }
    let season = parts[1].parse::<i32>().unwrap_or(0);
    let episode = parts[2].parse::<i32>().unwrap_or(0);
    if season == 0 || episode == 0 {
        return false;
    }
    let path_lower = path.to_lowercase();
    let pattern_s_e = format!("s{:02}e{:02}", season, episode);
    let pattern_sx_ex = format!("{}x{:02}", season, episode);
    let pattern_ep = format!("e{:02}", episode);
    path_lower.contains(&pattern_s_e)
        || path_lower.contains(&pattern_sx_ex)
        || path_lower.contains(&pattern_ep)
}
