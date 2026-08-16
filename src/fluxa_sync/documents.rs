use serde_json::{Value, json};

pub(super) fn hash_payload(value: &Value) -> String {
    let text = value.to_string();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub(super) fn document(entity: &str, key: &str, payload: Value) -> Value {
    json!({ "entity_type": entity, "key": key, "payload": payload })
}

pub(super) fn entries(value: Option<&Value>) -> Vec<(String, Value)> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(key, item)| (key.clone(), item.clone()))
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn sparse_against(value: &Value, defaults: &Value) -> Value {
    let (Some(fields), Some(base)) = (value.as_object(), defaults.as_object()) else {
        return value.clone();
    };
    let mut diff = serde_json::Map::new();
    for (key, current) in fields {
        if base.get(key) != Some(current) {
            diff.insert(key.clone(), current.clone());
        }
    }
    Value::Object(diff)
}

pub(super) fn merged_with(sparse: &Value, defaults: &Value) -> Value {
    let Some(base) = defaults.as_object() else {
        return sparse.clone();
    };
    let mut merged = base.clone();
    if let Some(fields) = sparse.as_object() {
        for (key, value) in fields {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

fn push_single(documents: &mut Vec<Value>, args: &Value, field: &str, entity: &str, key: &str) {
    if let Some(value) = args.get(field).filter(|value| !value.is_null()) {
        documents.push(document(entity, key, value.clone()));
    }
}

pub(crate) fn documents_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let mut documents: Vec<Value> = Vec::new();

    for (key, value) in entries(args.get("progress")) {
        documents.push(document("watch_progress", &key, value));
    }

    for (key, value) in entries(args.get("watched")) {
        if value.as_bool() == Some(true) {
            documents.push(document(
                "watched_history",
                &format!("video:{key}"),
                json!({ "watched": true }),
            ));
        }
    }

    for (key, value) in entries(args.get("lastWatched")) {
        documents.push(document("watched_history", &format!("series:{key}"), value));
    }

    for (status, items) in entries(args.get("library")) {
        for item in items.as_array().cloned().unwrap_or_default() {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            documents.push(document(
                "library",
                id,
                json!({ "status": status, "item": item }),
            ));
        }
    }

    for collection in args
        .get("collections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let Some(id) = collection.get("id").and_then(Value::as_str) else {
            continue;
        };
        documents.push(document("collections", id, collection.clone()));
    }

    push_single(&mut documents, &args, "addons", "addons", "local");
    push_single(&mut documents, &args, "plugins", "plugins", "local");
    push_single(&mut documents, &args, "profile", "settings", "profile");

    if let Some(settings) = args.get("settings").filter(|value| !value.is_null()) {
        let defaults = args.get("settingsDefaults").cloned().unwrap_or(Value::Null);
        let sparse = sparse_against(settings, &defaults);
        if sparse.as_object().is_none_or(|diff| !diff.is_empty()) {
            documents.push(document("settings", "app", sparse));
        }
    }

    serde_json::to_string(&json!({ "documents": documents })).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload_for(result: &str, entity: &str, key: &str) -> Value {
        let parsed: Value = serde_json::from_str(result).expect("valid json");
        parsed
            .get("documents")
            .and_then(Value::as_array)
            .expect("documents")
            .iter()
            .find(|document| {
                document.get("entity_type").and_then(Value::as_str) == Some(entity)
                    && document.get("key").and_then(Value::as_str) == Some(key)
            })
            .and_then(|document| document.get("payload"))
            .cloned()
            .unwrap_or(Value::Null)
    }

    #[test]
    fn settings_matching_defaults_are_not_stored() {
        let result = documents_json(
            &json!({
                "settings": { "autoplay": true, "theme": "dark", "volume": 40 },
                "settingsDefaults": { "autoplay": true, "theme": "dark", "volume": 100 },
            })
            .to_string(),
        )
        .expect("plan");

        assert_eq!(payload_for(&result, "settings", "app"), json!({ "volume": 40 }));
    }

    #[test]
    fn settings_back_at_their_defaults_stop_being_a_document() {
        let result = documents_json(
            &json!({
                "settings": { "autoplay": true, "volume": 100 },
                "settingsDefaults": { "autoplay": true, "volume": 100 },
            })
            .to_string(),
        )
        .expect("plan");
        let parsed: Value = serde_json::from_str(&result).expect("valid json");

        assert!(
            parsed
                .get("documents")
                .and_then(Value::as_array)
                .expect("documents")
                .is_empty()
        );
    }

    #[test]
    fn settings_survive_a_default_that_changes_later() {
        let stored = json!({ "volume": 40 });
        let effective = merged_with(&stored, &json!({ "autoplay": false, "volume": 100 }));

        assert_eq!(effective.get("autoplay"), Some(&json!(false)));
        assert_eq!(effective.get("volume"), Some(&json!(40)));
    }
}
