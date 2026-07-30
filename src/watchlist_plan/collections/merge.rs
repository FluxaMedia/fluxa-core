use serde_json::{Value, json};

pub(crate) fn collection_merge_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let existing = args.get("existing")?.as_array()?;
    let incoming = args.get("incoming")?.as_array()?;
    let mut merged = existing.clone();
    let mut ids: std::collections::HashSet<&str> = existing
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .collect();
    merged.extend(
        incoming
            .iter()
            .filter(|item| {
                item.get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| ids.insert(id))
            })
            .cloned(),
    );
    serde_json::to_string(&merged).ok()
}

pub(crate) fn collection_folder_items_plan_json(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let folder = args.get("folder")?;
    let categories = args.get("categories")?.as_array()?;
    let remote_sources = folder
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|source| {
            matches!(
                source.get("provider").and_then(Value::as_str),
                Some("trakt" | "tmdb")
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let modern: Vec<Value> = folder
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|source| source.get("provider").and_then(Value::as_str) == Some("addon"))
        .cloned()
        .collect();
    let fallback = folder
        .get("catalogSources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let sources = if !modern.is_empty() {
        modern
    } else if !fallback.is_empty() {
        fallback
    } else {
        folder.get("catalogId").or_else(|| folder.get("catalog_id")).and_then(Value::as_str)
            .map(|id| vec![json!({"catalogId": id, "type": folder.get("type").or_else(|| folder.get("catalogType"))})]).unwrap_or_default()
    };
    let mut groups: Vec<Value> = Vec::new();
    for source in sources {
        let catalog_id = source
            .get("catalogId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(category) = categories.iter().find(|category| {
            category.get("id").and_then(Value::as_str) == Some(catalog_id)
                || category.get("catalogId").and_then(Value::as_str) == Some(catalog_id)
        }) else {
            continue;
        };
        let content_type = source.get("type").and_then(Value::as_str).unwrap_or("");
        let genre = source
            .get("genre")
            .or_else(|| folder.get("genre"))
            .and_then(Value::as_str);
        let selected: Vec<Value> = category
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|item| {
                genre.is_none_or(|target| {
                    item.get("genres")
                        .and_then(Value::as_array)
                        .is_some_and(|genres| {
                            genres
                                .iter()
                                .filter_map(Value::as_str)
                                .any(|value| value.eq_ignore_ascii_case(target))
                        })
                })
            })
            .cloned()
            .collect();
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.get("type").and_then(Value::as_str) == Some(content_type))
        {
            group.get_mut("items")?.as_array_mut()?.extend(selected);
        } else {
            groups.push(json!({"type": content_type, "items": selected}));
        }
    }
    let items: Vec<Value> = groups
        .iter()
        .flat_map(|group| {
            group
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .cloned()
        })
        .collect();
    serde_json::to_string(
        &json!({"items": items, "groups": groups, "remoteSources": remote_sources}),
    )
    .ok()
}
