use ::dolby_vision::rpu::dovi_rpu::DoviRpu;
use ::dolby_vision::rpu::rpu_data_nlq::DoviELType;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

const RPU_NAL_TYPE: u8 = 62;
const EL_NAL_TYPE_63: u8 = 63;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framing {
    AnnexB,
    LengthDelimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnhancementLayer {
    #[default]
    None,
    Mel,
    Fel,
}

impl From<Option<&DoviELType>> for EnhancementLayer {
    fn from(value: Option<&DoviELType>) -> Self {
        match value {
            Some(DoviELType::MEL) => EnhancementLayer::Mel,
            Some(DoviELType::FEL) => EnhancementLayer::Fel,
            None => EnhancementLayer::None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SampleRequest {
    #[serde(default)]
    sample_base64: String,
    framing: Framing,
    #[serde(default = "default_nal_length_size")]
    nal_length_size: u8,
    #[serde(default = "default_mode")]
    mode: u8,
}

fn default_nal_length_size() -> u8 {
    4
}

fn default_mode() -> u8 {
    2
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleTransformResult {
    ok: bool,
    changed: bool,
    output_base64: Option<String>,
    output_size: usize,
    rpu_found: u32,
    rpu_converted: u32,
    rpu_failed: u32,
    el_nals_dropped: u32,
    enhancement_layer: EnhancementLayer,
    error: Option<String>,
}

struct Nal<'a> {
    start_code_len: usize,
    bytes: &'a [u8],
    nal_type: u8,
    layer_id: u8,
}

fn nal_header(bytes: &[u8]) -> Option<(u8, u8)> {
    let &[b0, b1, ..] = bytes else { return None };
    let nal_type = (b0 >> 1) & 0x3F;
    let layer_id = ((b0 & 0x1) << 5) | (b1 >> 3);
    Some((nal_type, layer_id))
}

fn split_annex_b(data: &[u8]) -> Option<Vec<Nal<'_>>> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 2 < data.len() {
        if data.get(i..i + 3) == Some(&[0, 0, 1]) {
            if i > 0 && data.get(i - 1) == Some(&0) {
                starts.push((i - 1, 4usize));
            } else {
                starts.push((i, 3usize));
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    if starts.is_empty() {
        return None;
    }
    let mut nals = Vec::with_capacity(starts.len());
    for (idx, &(pos, code_len)) in starts.iter().enumerate() {
        let body_start = pos + code_len;
        let body_end = starts.get(idx + 1).map(|&(p, _)| p).unwrap_or(data.len());
        let body = data.get(body_start..body_end)?;
        let (nal_type, layer_id) = nal_header(body)?;
        nals.push(Nal {
            start_code_len: code_len,
            bytes: body,
            nal_type,
            layer_id,
        });
    }
    Some(nals)
}

fn split_length_delimited(data: &[u8], nal_length_size: u8) -> Option<Vec<Nal<'_>>> {
    let size = nal_length_size as usize;
    if !(1..=4).contains(&size) {
        return None;
    }
    let mut nals = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let prefix = data.get(i..i + size)?;
        let len = prefix
            .iter()
            .fold(0usize, |acc, &b| (acc << 8) | b as usize);
        let body_start = i + size;
        let body = data.get(body_start..body_start + len)?;
        let (nal_type, layer_id) = nal_header(body)?;
        nals.push(Nal {
            start_code_len: size,
            bytes: body,
            nal_type,
            layer_id,
        });
        i = body_start + len;
    }
    Some(nals)
}

fn write_length(out: &mut Vec<u8>, len: usize, size: usize) {
    for shift in (0..size).rev() {
        out.push(((len >> (shift * 8)) & 0xFF) as u8);
    }
}

pub fn process_dv_sample_json(input: &str) -> Option<String> {
    let request = match serde_json::from_str::<SampleRequest>(input) {
        Ok(request) => request,
        Err(error) => {
            return serde_json::to_string(&SampleTransformResult {
                ok: false,
                error: Some(error.to_string()),
                ..Default::default()
            })
            .ok();
        }
    };

    let sample = match BASE64.decode(request.sample_base64.as_bytes()) {
        Ok(bytes) => bytes,
        Err(error) => {
            return serde_json::to_string(&SampleTransformResult {
                ok: false,
                error: Some(error.to_string()),
                ..Default::default()
            })
            .ok();
        }
    };

    let nals = match request.framing {
        Framing::AnnexB => split_annex_b(&sample),
        Framing::LengthDelimited => split_length_delimited(&sample, request.nal_length_size),
    };

    let Some(nals) = nals else {
        return serde_json::to_string(&SampleTransformResult {
            ok: false,
            output_base64: Some(request.sample_base64),
            output_size: sample.len(),
            error: Some("failed_to_parse_nal_units".to_string()),
            ..Default::default()
        })
        .ok();
    };

    let mut rpu_found = 0u32;
    let mut rpu_failed = 0u32;
    let mut enhancement_layer = EnhancementLayer::None;
    let mut converted_rpu: Vec<Option<Vec<u8>>> = Vec::with_capacity(nals.len());
    let mut any_conversion_failed = false;

    for nal in &nals {
        if nal.nal_type == RPU_NAL_TYPE && nal.layer_id == 0 {
            rpu_found += 1;
            match DoviRpu::parse_unspec62_nalu(nal.bytes) {
                Ok(mut rpu) => {
                    enhancement_layer = EnhancementLayer::from(rpu.el_type.as_ref());
                    match rpu
                        .convert_with_mode(request.mode)
                        .and_then(|()| rpu.write_hevc_unspec62_nalu())
                    {
                        Ok(bytes) => converted_rpu.push(Some(bytes)),
                        Err(_) => {
                            rpu_failed += 1;
                            any_conversion_failed = true;
                            converted_rpu.push(None);
                        }
                    }
                }
                Err(_) => {
                    rpu_failed += 1;
                    any_conversion_failed = true;
                    converted_rpu.push(None);
                }
            }
        } else {
            converted_rpu.push(None);
        }
    }

    if any_conversion_failed {
        return serde_json::to_string(&SampleTransformResult {
            ok: true,
            changed: false,
            output_base64: Some(request.sample_base64),
            output_size: sample.len(),
            rpu_found,
            rpu_converted: 0,
            rpu_failed,
            el_nals_dropped: 0,
            enhancement_layer,
            error: Some("rpu_conversion_failed_original_sample_kept".to_string()),
        })
        .ok();
    }

    let mut output = Vec::with_capacity(sample.len());
    let mut el_nals_dropped = 0u32;
    let mut rpu_converted = 0u32;
    let is_annex_b = matches!(request.framing, Framing::AnnexB);

    for (nal, replacement) in nals.iter().zip(converted_rpu.iter()) {
        let is_el = nal.layer_id != 0 || nal.nal_type == EL_NAL_TYPE_63;
        if is_el {
            el_nals_dropped += 1;
            continue;
        }
        let bytes = if let Some(replacement) = replacement {
            rpu_converted += 1;
            replacement.as_slice()
        } else {
            nal.bytes
        };
        if is_annex_b {
            output.extend_from_slice(&[0, 0, 0, 1]);
            output.extend_from_slice(bytes);
        } else {
            write_length(&mut output, bytes.len(), nal.start_code_len);
            output.extend_from_slice(bytes);
        }
    }

    let changed = rpu_converted > 0 || el_nals_dropped > 0;
    let output_base64 = BASE64.encode(&output);
    let output_size = output.len();

    serde_json::to_string(&SampleTransformResult {
        ok: true,
        changed,
        output_base64: Some(output_base64),
        output_size,
        rpu_found,
        rpu_converted,
        rpu_failed: 0,
        el_nals_dropped,
        enhancement_layer,
        error: None,
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el_nal(layer_id: u8) -> Vec<u8> {
        let b0 = 0x02u8 | ((layer_id >> 5) & 0x1);
        let b1 = (layer_id & 0x1F) << 3;
        vec![b0, b1, 0xAA, 0xBB]
    }

    fn annex_b_sample(nals: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        for nal in nals {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
        }
        out
    }

    #[test]
    fn el_nal_with_nonzero_layer_id_is_dropped() {
        let vcl = vec![0x26, 0x01, 0x00, 0x11];
        let el = el_nal(1);
        let sample = annex_b_sample(&[vcl.clone(), el]);
        let request = format!(
            r#"{{"sampleBase64":"{}","framing":"annex_b"}}"#,
            BASE64.encode(&sample)
        );
        let result: serde_json::Value =
            serde_json::from_str(&process_dv_sample_json(&request).unwrap()).unwrap();
        assert_eq!(result["elNalsDropped"], 1);
        assert_eq!(result["changed"], true);
        let out = BASE64
            .decode(result["outputBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(out, annex_b_sample(&[vcl]));
    }

    #[test]
    fn nal_type_63_is_dropped_even_at_layer_zero() {
        let vcl = vec![0x26, 0x01, 0x00, 0x11];
        let el = vec![0x7E, 0x01, 0xAA];
        let sample = annex_b_sample(&[vcl.clone(), el]);
        let request = format!(
            r#"{{"sampleBase64":"{}","framing":"annex_b"}}"#,
            BASE64.encode(&sample)
        );
        let result: serde_json::Value =
            serde_json::from_str(&process_dv_sample_json(&request).unwrap()).unwrap();
        assert_eq!(result["elNalsDropped"], 1);
        let out = BASE64
            .decode(result["outputBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(out, annex_b_sample(&[vcl]));
    }

    #[test]
    fn no_el_or_rpu_present_gives_unchanged_output() {
        let vcl = vec![0x26, 0x01, 0x00, 0x11];
        let sample = annex_b_sample(&[vcl.clone()]);
        let request = format!(
            r#"{{"sampleBase64":"{}","framing":"annex_b"}}"#,
            BASE64.encode(&sample)
        );
        let result: serde_json::Value =
            serde_json::from_str(&process_dv_sample_json(&request).unwrap()).unwrap();
        assert_eq!(result["changed"], false);
        assert_eq!(result["elNalsDropped"], 0);
        assert_eq!(result["rpuFound"], 0);
    }

    #[test]
    fn malformed_sample_falls_back_to_original() {
        let request = format!(
            r#"{{"sampleBase64":"{}","framing":"annex_b"}}"#,
            BASE64.encode(b"not a valid nal stream at all")
        );
        let result: serde_json::Value =
            serde_json::from_str(&process_dv_sample_json(&request).unwrap()).unwrap();
        assert_eq!(result["ok"], false);
        assert!(result["outputBase64"].is_string());
    }

    #[test]
    fn length_delimited_framing_round_trips_size_prefix() {
        let vcl = vec![0x26, 0x01, 0x00, 0x11];
        let mut sample = Vec::new();
        write_length(&mut sample, vcl.len(), 4);
        sample.extend_from_slice(&vcl);
        let request = format!(
            r#"{{"sampleBase64":"{}","framing":"length_delimited","nalLengthSize":4}}"#,
            BASE64.encode(&sample)
        );
        let result: serde_json::Value =
            serde_json::from_str(&process_dv_sample_json(&request).unwrap()).unwrap();
        assert_eq!(result["changed"], false);
        let out = BASE64
            .decode(result["outputBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(out, sample);
    }
}
