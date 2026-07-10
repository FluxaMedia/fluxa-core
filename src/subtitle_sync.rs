use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubtitleSyncRequest {
    #[serde(default)]
    subtitle_text: String,
    #[serde(default)]
    subtitle_cues: Vec<Interval>,
    #[serde(default)]
    speech_intervals: Vec<Interval>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Interval {
    start: f64,
    end: f64,
}

pub(crate) fn estimate_subtitle_delay_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<SubtitleSyncRequest>(request_json).ok()?;
    let subtitle_cues = if request.subtitle_text.trim().is_empty() {
        normalize_intervals(request.subtitle_cues)
    } else {
        parse_subtitle_cues(&request.subtitle_text)
    };
    let speech_intervals = normalize_intervals(request.speech_intervals);
    if subtitle_cues.len() < 3 || speech_intervals.len() < 3 {
        return None;
    }

    let mut scores = Vec::new();
    for step in -120..=120 {
        let delay = step as f64 * 0.25;
        scores.push((delay, overlap_score(&subtitle_cues, &speech_intervals, delay)));
    }
    let (best_delay, best_score) = scores.iter().copied().max_by(|left, right| left.1.total_cmp(&right.1))?;
    let second_score = scores.iter().filter(|(delay, _)| (delay - best_delay).abs() >= 1.0).map(|(_, score)| *score).max_by(f64::total_cmp).unwrap_or(0.0);

    let refinement_start = best_delay - 0.25;
    let refinement_end = best_delay + 0.25;
    let mut refined_delay = best_delay;
    let mut refined_score = best_score;
    for step in 0..=20 {
        let delay = refinement_start + step as f64 * (refinement_end - refinement_start) / 20.0;
        let score = overlap_score(&subtitle_cues, &speech_intervals, delay);
        if score > refined_score {
            refined_score = score;
            refined_delay = delay;
        }
    }

    let confidence = ((refined_score - second_score.max(0.0)) / refined_score.max(0.001)).clamp(0.0, 1.0);
    if refined_score < 0.18 || confidence < 0.08 {
        return None;
    }
    Some(json!({
        "delaySeconds": (refined_delay * 10.0).round() / 10.0,
        "confidence": confidence,
    }).to_string())
}

fn parse_subtitle_cues(text: &str) -> Vec<Interval> {
    text.lines().filter_map(|line| {
        if let Some((start, rest)) = line.split_once("-->") {
            let end = rest.split_whitespace().next()?;
            let start = parse_timestamp(start.trim())?;
            let end = parse_timestamp(end.trim())?;
            return (end > start).then_some(Interval { start, end });
        }
        let dialogue = line.trim().strip_prefix("Dialogue:")?;
        let fields = dialogue.split(',').collect::<Vec<_>>();
        let start = parse_timestamp(fields.get(1)?.trim())?;
        let end = parse_timestamp(fields.get(2)?.trim())?;
        (end > start).then_some(Interval { start, end })
    }).collect()
}

fn parse_timestamp(value: &str) -> Option<f64> {
    let normalized = value.replace(',', ".");
    let segments = normalized.split(':').collect::<Vec<_>>();
    let seconds = segments.last()?.parse::<f64>().ok()?;
    let minutes = segments.get(segments.len().checked_sub(2)?)?.parse::<f64>().ok()?;
    let hours = if segments.len() == 3 { segments.first()?.parse::<f64>().ok()? } else { 0.0 };
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn normalize_intervals(intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.into_iter().filter(|item| item.start.is_finite() && item.end.is_finite() && item.end > item.start).collect()
}

fn overlap_score(subtitles: &[Interval], speech: &[Interval], delay: f64) -> f64 {
    let subtitle_duration = subtitles.iter().map(|cue| cue.end - cue.start).sum::<f64>().max(0.001);
    let overlap = subtitles.iter().map(|cue| {
        let start = cue.start + delay;
        let end = cue.end + delay;
        speech.iter().map(|activity| (end.min(activity.end) - start.max(activity.start)).max(0.0)).sum::<f64>()
    }).sum::<f64>();
    let boundary_alignment = subtitles.iter().map(|cue| {
        let time = cue.start + delay;
        let nearest = speech.iter().flat_map(|activity| [activity.start, activity.end]).map(|boundary| (time - boundary).abs()).fold(f64::INFINITY, f64::min);
        (-nearest / 0.5).exp()
    }).sum::<f64>() / subtitles.len() as f64;
    overlap / subtitle_duration * 0.7 + boundary_alignment * 0.3
}

#[cfg(test)]
mod tests {
    use super::estimate_subtitle_delay_json;
    use serde_json::Value;

    #[test]
    fn estimates_delay_from_subtitle_and_speech_timelines() {
        let result = estimate_subtitle_delay_json(r#"{
          "subtitleText":"00:00:10,000 --> 00:00:10,800\na\n\n00:00:20,000 --> 00:00:20,800\nb\n\n00:00:30,000 --> 00:00:30,800\nc\n",
          "speechIntervals":[{"start":12.0,"end":12.8},{"start":22.0,"end":22.8},{"start":32.0,"end":32.8}]
        }"#).expect("sync estimate");
        let value: Value = serde_json::from_str(&result).expect("valid result");
        assert!((value["delaySeconds"].as_f64().expect("delay") - 2.0).abs() <= 0.1);
    }

    #[test]
    fn parses_ass_dialogue_cues() {
        let result = estimate_subtitle_delay_json(r#"{
          "subtitleText":"[Events]\nDialogue: 0,0:00:10.00,0:00:10.80,Default,,0,0,0,,a\nDialogue: 0,0:00:20.00,0:00:20.80,Default,,0,0,0,,b\nDialogue: 0,0:00:30.00,0:00:30.80,Default,,0,0,0,,c",
          "speechIntervals":[{"start":12.0,"end":12.8},{"start":22.0,"end":22.8},{"start":32.0,"end":32.8}]
        }"#).expect("sync estimate");
        let value: Value = serde_json::from_str(&result).expect("valid result");
        assert!((value["delaySeconds"].as_f64().expect("delay") - 2.0).abs() <= 0.1);
    }
}
