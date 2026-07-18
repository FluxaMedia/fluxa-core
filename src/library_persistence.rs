use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryItem<'a> {
    media_id: &'a str,
    status: &'static str,
    value: &'a Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyedValue<'a> {
    key: &'a str,
    value: &'a Value,
}

pub(crate) fn progress_entries_json(document_json: &str) -> String {
    object_entries(document_json, "progress")
}

pub(crate) fn library_items_json(document_json: &str) -> String {
    let document: Value = match serde_json::from_str(document_json) {
        Ok(document) => document,
        Err(_) => return "[]".to_string(),
    };
    let mut entries = Vec::new();
    for status in ["watchlist", "completed", "dropped"] {
        for item in document
            .get(status)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(media_id) = item.get("id").and_then(Value::as_str) {
                entries.push(LibraryItem {
                    media_id,
                    status,
                    value: item,
                });
            }
        }
    }
    serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) fn watched_video_ids_json(document_json: &str) -> String {
    let document: Value = match serde_json::from_str(document_json) {
        Ok(document) => document,
        Err(_) => return "[]".to_string(),
    };
    let ids = document
        .get("watched")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(video_id, watched)| (watched.as_bool() == Some(true)).then_some(video_id))
        .collect::<Vec<_>>();
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) fn last_watched_entries_json(document_json: &str) -> String {
    object_entries(document_json, "lastWatchedEpisodes")
}

pub(crate) fn continue_watching_entries_json(document_json: &str) -> String {
    let document: Value = match serde_json::from_str(document_json) {
        Ok(document) => document,
        Err(_) => return "[]".to_string(),
    };
    let entries = document
        .get("externalContinueWatching")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(KeyedValue {
                key: item.get("id")?.as_str()?,
                value: item,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
}

fn object_entries(document_json: &str, field: &str) -> String {
    let document: Value = match serde_json::from_str(document_json) {
        Ok(document) => document,
        Err(_) => return "[]".to_string(),
    };
    let entries = document
        .get(field)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(key, value)| KeyedValue { key, value })
        .collect::<Vec<_>>();
    serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_all_legacy_library_domains() {
        let document = r#"{
            "progress":{"movie:1":{"position":12}},
            "watchlist":[{"id":"movie:1"}],
            "completed":[{"id":"movie:2"}],
            "dropped":[],
            "watched":{"video:1":true,"video:2":false},
            "lastWatchedEpisodes":{"series:1":{"id":"episode:2"}},
            "externalContinueWatching":[{"id":"movie:3"}]
        }"#;
        assert!(progress_entries_json(document).contains("movie:1"));
        assert!(library_items_json(document).contains("completed"));
        assert_eq!(watched_video_ids_json(document), r#"["video:1"]"#);
        assert!(last_watched_entries_json(document).contains("series:1"));
        assert!(continue_watching_entries_json(document).contains("movie:3"));
    }
}
