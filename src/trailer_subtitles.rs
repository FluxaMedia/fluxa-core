use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

fn language(value: Option<&str>) -> Option<String> {
    let value = value?.trim().to_lowercase();
    if value.is_empty() || value == "none" {
        return None;
    }
    value.split(['-', '_']).next().map(str::to_string)
}

pub(crate) fn trailer_subtitle_selection_plan_json(input: &str) -> Option<String> {
    let value: Value = serde_json::from_str(input).ok()?;
    let tracks = value.get("tracks")?.as_array()?;
    if tracks.is_empty() {
        return Some("null".to_string());
    }
    let mut wanted = Vec::new();
    for candidate in ["preferred", "secondary", "systemLanguage"] {
        if let Some(value) = language(value.get(candidate).and_then(Value::as_str)) {
            if !wanted.contains(&value) {
                wanted.push(value);
            }
        }
    }
    if !wanted.iter().any(|value| value == "en") {
        wanted.push("en".to_string());
    }
    let selected = tracks.iter().enumerate().max_by_key(|(index, track)| {
        let track_language = language(track.get("languageTag").and_then(Value::as_str));
        let wanted_index =
            track_language.and_then(|language| wanted.iter().position(|value| value == &language));
        let preferred = wanted_index
            .map(|position| 1000_i64 - position as i64 * 100)
            .unwrap_or_default();
        let english_label = (wanted_index.is_none()
            && track
                .get("label")
                .and_then(Value::as_str)
                .is_some_and(|label| label.to_lowercase().contains("english")))
            as i64
            * 250;
        let human = (!track
            .get("isAuto")
            .and_then(Value::as_bool)
            .unwrap_or(false)) as i64
            * 25;
        (preferred + english_label + human, std::cmp::Reverse(*index))
    });
    selected.map(|(_, track)| track.to_string())
}

pub(crate) fn normalize_trailer_subtitle_url_json(input: &str) -> Option<String> {
    let value: Value = serde_json::from_str(input).ok()?;
    let raw = value.get("url")?.as_str()?;
    let normalized = match url::Url::parse(raw) {
        Ok(mut url) => {
            url.query_pairs_mut().append_pair("fmt", "vtt");
            url.to_string()
        }
        Err(_) => raw.to_string(),
    };
    serde_json::to_string(&normalized).ok()
}

fn timing_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r"(?m)^\s*([^\s]+)\s+-->\s+([^\s]+).*$").expect("valid timing regex")
    })
}

fn tag_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"<[^>]+>").expect("valid tag regex"))
}

fn timed_text_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE
        .get_or_init(|| Regex::new(r#"(?s)<p\b([^>]*)>(.*?)</p>"#).expect("valid timed text regex"))
}

fn timed_text_attribute_regex() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r#"\b([td])=\"(\d+)\""#).expect("valid timed text attribute regex")
    })
}

fn parse_time(raw: &str) -> Option<f64> {
    let normalized = raw.trim().replace(',', ".");
    let mut parts = normalized.split(':').rev().map(str::parse::<f64>);
    let seconds = parts.next()?.ok()?;
    let minutes = parts.next().transpose().ok()?.unwrap_or_default();
    let hours = parts.next().transpose().ok()?.unwrap_or_default();
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn parse_vtt(input: &str) -> Vec<Value> {
    input
        .replace('\r', "")
        .split("\n\n")
        .filter_map(|block| {
            let capture = timing_regex().captures(block)?;
            let start = parse_time(capture.get(1)?.as_str())?;
            let end = parse_time(capture.get(2)?.as_str())?;
            let timing_end = capture.get(0)?.end();
            let text =
                decode_entities(tag_regex().replace_all(block.get(timing_end..)?, "").trim());
            (!text.is_empty()).then(|| json!({ "start": start, "end": end, "text": text }))
        })
        .collect()
}

fn parse_timed_text(input: &str) -> Vec<Value> {
    timed_text_regex()
        .captures_iter(input)
        .filter_map(|capture| {
            let attributes = capture.get(1)?.as_str();
            let attribute = |name: &str| {
                timed_text_attribute_regex()
                    .captures_iter(attributes)
                    .find_map(|item| {
                        (item.get(1)?.as_str() == name)
                            .then(|| item.get(2)?.as_str().parse::<f64>().ok())
                            .flatten()
                    })
            };
            let start = attribute("t")? / 1000.0;
            let duration = attribute("d")? / 1000.0;
            let text =
                decode_entities(tag_regex().replace_all(capture.get(2)?.as_str(), "").trim());
            (!text.is_empty())
                .then(|| json!({ "start": start, "end": start + duration, "text": text }))
        })
        .collect()
}

pub(crate) fn parse_trailer_subtitle_cues_json(input: &str) -> Option<String> {
    let value: Value = serde_json::from_str(input).ok()?;
    let body = value.get("body")?.as_str()?;
    let trimmed = body.trim_start();
    let cues = if trimmed.starts_with("<?xml") || trimmed.starts_with("<timedtext") {
        parse_timed_text(body)
    } else {
        parse_vtt(body)
    };
    serde_json::to_string(&cues).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_preferred_human_subtitle() {
        let result = trailer_subtitle_selection_plan_json(r#"{"tracks":[{"languageTag":"en","label":"English","isAuto":true},{"languageTag":"tr-TR","label":"Türkçe","isAuto":false}],"preferred":"tr"}"#).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&result).unwrap()["languageTag"],
            "tr-TR"
        );
    }

    #[test]
    fn parses_vtt_and_timed_text() {
        let vtt = parse_trailer_subtitle_cues_json(
            r#"{"body":"WEBVTT\n\n00:00:01.000 --> 00:00:02.500\nHello &amp; world"}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&vtt).unwrap()[0]["text"],
            "Hello & world"
        );
        let xml = parse_trailer_subtitle_cues_json(r#"{"body":"<?xml version=\"1.0\"?><timedtext><p t=\"1000\" d=\"500\">Hi</p></timedtext>"}"#).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&xml).unwrap()[0]["end"], 1.5);
    }
}
