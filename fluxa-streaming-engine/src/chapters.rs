// Standalone Matroska Chapters reader.
//
// ExoPlayer's MatroskaExtractor doesn't surface the Chapters element, so for the
// ExoPlayer backend we read a prefix of the file ourselves and walk the EBML tree
// far enough to find Segment -> Chapters -> EditionEntry -> ChapterAtom, without
// needing a full demuxer. Chapters always precede Cluster data in every muxer we
// care about (mkvmerge, ffmpeg), so scanning stops as soon as a Cluster is hit.

use crate::dv_rewrite::try_parse_ebml_header;

const ID_EBML_HEADER: u64 = 0x1A45DFA3;
const ID_SEGMENT: u64 = 0x1853_8067;
const ID_CLUSTER: u64 = 0x1F43_B675;
const ID_CHAPTERS: u64 = 0x1043_A770;
const ID_EDITION_ENTRY: u64 = 0x45B9;
const ID_CHAPTER_ATOM: u64 = 0xB6;
const ID_CHAPTER_TIME_START: u64 = 0x91;
const ID_CHAPTER_DISPLAY: u64 = 0x80;
const ID_CHAPTER_STRING: u64 = 0x85;
const UNKNOWN_SIZE: u64 = u64::MAX;

#[derive(Debug, PartialEq)]
pub(crate) struct MkvChapter {
    pub title: String,
    pub start_ms: u64,
}

/// Read `(id, header_len, content_start, content_end)` for the element at `pos`,
/// clamping an unknown-size element's content to `scan_end`.
fn read_element(buf: &[u8], pos: usize, scan_end: usize) -> Option<(u64, usize, usize, usize)> {
    let (id, size, header_len) = try_parse_ebml_header(&buf[pos..scan_end.min(buf.len())])?;
    let content_start = pos + header_len;
    let content_end = if size == UNKNOWN_SIZE {
        scan_end
    } else {
        (content_start + size as usize).min(scan_end)
    };
    Some((id, header_len, content_start, content_end))
}

fn parse_chapter_display(buf: &[u8], mut pos: usize, end: usize) -> Option<String> {
    while pos < end {
        let (id, _, content_start, content_end) = read_element(buf, pos, end)?;
        if id == ID_CHAPTER_STRING {
            return String::from_utf8(buf[content_start..content_end].to_vec()).ok();
        }
        pos = content_end.max(pos + 1);
    }
    None
}

fn read_uint_be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64)
}

fn parse_chapter_atom(buf: &[u8], mut pos: usize, end: usize) -> Option<MkvChapter> {
    let mut start_ms: Option<u64> = None;
    let mut title: Option<String> = None;
    while pos < end {
        let (id, _, content_start, content_end) = read_element(buf, pos, end)?;
        match id {
            ID_CHAPTER_TIME_START => {
                let raw_ns = read_uint_be(&buf[content_start..content_end]);
                start_ms = Some(raw_ns / 1_000_000);
            }
            ID_CHAPTER_DISPLAY if title.is_none() => {
                title = parse_chapter_display(buf, content_start, content_end);
            }
            _ => {}
        }
        pos = content_end.max(pos + 1);
    }
    start_ms.map(|ms| MkvChapter { title: title.unwrap_or_default(), start_ms: ms })
}

fn parse_edition_entry(buf: &[u8], mut pos: usize, end: usize, out: &mut Vec<MkvChapter>) {
    while pos < end {
        let Some((id, _, content_start, content_end)) = read_element(buf, pos, end) else { return };
        if id == ID_CHAPTER_ATOM {
            if let Some(chapter) = parse_chapter_atom(buf, content_start, content_end) {
                out.push(chapter);
            }
        }
        pos = content_end.max(pos + 1);
    }
}

fn parse_chapters_element(buf: &[u8], mut pos: usize, end: usize) -> Vec<MkvChapter> {
    let mut chapters = Vec::new();
    while pos < end {
        let Some((id, _, content_start, content_end)) = read_element(buf, pos, end) else { break };
        if id == ID_EDITION_ENTRY {
            parse_edition_entry(buf, content_start, content_end, &mut chapters);
        }
        pos = content_end.max(pos + 1);
    }
    chapters
}

/// Scan the direct children of `Segment` looking for `Chapters`, stopping as soon
/// as a `Cluster` is reached (chapter metadata always precedes frame data).
fn scan_segment(buf: &[u8], mut pos: usize, end: usize) -> Vec<MkvChapter> {
    while pos < end {
        let Some((id, _, content_start, content_end)) = read_element(buf, pos, end) else { break };
        if id == ID_CLUSTER {
            break;
        }
        if id == ID_CHAPTERS {
            return parse_chapters_element(buf, content_start, content_end);
        }
        pos = content_end.max(pos + 1);
    }
    Vec::new()
}

/// Parse chapters from a prefix of an MKV/WebM file. `buf` only needs to cover the
/// EBML header, Segment header, and whatever metadata elements precede the first
/// Cluster — a multi-megabyte prefix is normally more than enough.
pub(crate) fn parse_mkv_chapters(buf: &[u8]) -> Vec<MkvChapter> {
    let mut pos = 0usize;
    let end = buf.len();
    if let Some((id, _, _content_start, content_end)) = read_element(buf, pos, end) {
        if id == ID_EBML_HEADER {
            pos = content_end;
        }
    }
    while pos < end {
        let Some((id, _, content_start, content_end)) = read_element(buf, pos, end) else { break };
        if id == ID_SEGMENT {
            return scan_segment(buf, content_start, content_end);
        }
        pos = content_end.max(pos + 1);
    }
    Vec::new()
}

pub(crate) fn parse_mkv_chapters_json(buf: &[u8]) -> String {
    let chapters = parse_mkv_chapters(buf);
    let arr: Vec<serde_json::Value> = chapters
        .iter()
        .map(|c| serde_json::json!({ "title": c.title, "startMs": c.start_ms }))
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dv_rewrite::encode_ebml_element;

    fn chapter_atom(start_ms: u64, title: &str) -> Vec<u8> {
        let time_start = encode_ebml_element(ID_CHAPTER_TIME_START, &(start_ms * 1_000_000).to_be_bytes());
        let chapter_string = encode_ebml_element(ID_CHAPTER_STRING, title.as_bytes());
        let display = encode_ebml_element(ID_CHAPTER_DISPLAY, &chapter_string);
        let mut content = Vec::new();
        content.extend_from_slice(&time_start);
        content.extend_from_slice(&display);
        encode_ebml_element(ID_CHAPTER_ATOM, &content)
    }

    fn segment_with_chapters(chapters: &[(u64, &str)]) -> Vec<u8> {
        let atoms: Vec<u8> = chapters.iter().flat_map(|(ms, title)| chapter_atom(*ms, title)).collect();
        let edition_entry = encode_ebml_element(ID_EDITION_ENTRY, &atoms);
        let chapters_elem = encode_ebml_element(ID_CHAPTERS, &edition_entry);
        encode_ebml_element(ID_SEGMENT, &chapters_elem)
    }

    #[test]
    fn parses_single_chapter() {
        let segment = segment_with_chapters(&[(0, "OP")]);
        let chapters = parse_mkv_chapters(&segment);
        assert_eq!(chapters, vec![MkvChapter { title: "OP".to_string(), start_ms: 0 }]);
    }

    #[test]
    fn parses_multiple_chapters_in_order() {
        let segment = segment_with_chapters(&[(0, "OP"), (90_000, "Episode"), (1_320_000, "ED")]);
        let chapters = parse_mkv_chapters(&segment);
        assert_eq!(
            chapters,
            vec![
                MkvChapter { title: "OP".to_string(), start_ms: 0 },
                MkvChapter { title: "Episode".to_string(), start_ms: 90_000 },
                MkvChapter { title: "ED".to_string(), start_ms: 1_320_000 },
            ]
        );
    }

    #[test]
    fn stops_at_cluster_without_chapters() {
        let cluster = encode_ebml_element(ID_CLUSTER, &[0x00, 0x01, 0x02]);
        let segment = encode_ebml_element(ID_SEGMENT, &cluster);
        assert!(parse_mkv_chapters(&segment).is_empty());
    }

    #[test]
    fn chapters_before_cluster_are_still_found() {
        let chapters_elem = {
            let atom = chapter_atom(0, "Intro");
            let edition_entry = encode_ebml_element(ID_EDITION_ENTRY, &atom);
            encode_ebml_element(ID_CHAPTERS, &edition_entry)
        };
        let cluster = encode_ebml_element(ID_CLUSTER, &[0x00]);
        let mut segment_content = Vec::new();
        segment_content.extend_from_slice(&chapters_elem);
        segment_content.extend_from_slice(&cluster);
        let segment = encode_ebml_element(ID_SEGMENT, &segment_content);

        let chapters = parse_mkv_chapters(&segment);
        assert_eq!(chapters, vec![MkvChapter { title: "Intro".to_string(), start_ms: 0 }]);
    }

    #[test]
    fn handles_truncated_buffer_without_panicking() {
        let segment = segment_with_chapters(&[(0, "OP")]);
        for cut in 0..segment.len() {
            let _ = parse_mkv_chapters(&segment[..cut]);
        }
    }

    #[test]
    fn empty_buffer_returns_no_chapters() {
        assert!(parse_mkv_chapters(&[]).is_empty());
    }

    #[test]
    fn json_output_shape() {
        let segment = segment_with_chapters(&[(0, "OP")]);
        let json = parse_mkv_chapters_json(&segment);
        assert_eq!(json, r#"[{"startMs":0,"title":"OP"}]"#);
    }
}
