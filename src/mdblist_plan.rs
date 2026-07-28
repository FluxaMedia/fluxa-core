use serde_json::{Value, json};

const MDBLIST_API_BASE_URL: &str = "https://api.mdblist.com";

pub(crate) fn mdblist_bearer(token: &str) -> String {
    format!("Bearer {token}")
}

pub(crate) fn mdblist_device_poll_outcome(body_json: &str) -> &'static str {
    let Ok(body) = serde_json::from_str::<Value>(body_json) else {
        return "success";
    };
    match body.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => "pending",
        Some("slow_down") => "slow_down",
        Some("access_denied") => "denied",
        Some("expired_token") => "expired",
        Some(_) => "error",
        None => "success",
    }
}

fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn build_url(path: &str, params: &[(String, String)]) -> String {
    let mut url = format!("{MDBLIST_API_BASE_URL}{path}");
    let mut first = true;
    for (key, value) in params {
        if value.is_empty() {
            continue;
        }
        url.push(if first { '?' } else { '&' });
        first = false;
        url.push_str(key);
        url.push('=');
        url.push_str(&encode_query(value));
    }
    url
}

fn plan(method: &str, url: String, body: Option<Value>) -> Option<String> {
    serde_json::to_string(&json!({ "method": method, "url": url, "body": body })).ok()
}

fn value_to_query_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn extract_query(args: &Value, keys: &[&str]) -> Vec<(String, String)> {
    keys.iter()
        .filter_map(|&key| {
            let raw = args.get(key)?;
            Some((key.to_string(), value_to_query_string(raw)?))
        })
        .collect()
}

fn extract_repeated(args: &Value, key: &str, params: &mut Vec<(String, String)>) {
    match args.get(key) {
        Some(Value::Array(values)) => {
            for value in values {
                if let Some(s) = value_to_query_string(value) {
                    params.push((key.to_string(), s));
                }
            }
        }
        Some(other) => {
            if let Some(s) = value_to_query_string(other) {
                params.push((key.to_string(), s));
            }
        }
        None => {}
    }
}

const LIST_ITEMS_QUERY_KEYS: &[&str] = &[
    "cursor",
    "limit",
    "offset",
    "append_to_response",
    "extended",
    "mediatype",
    "filter_title",
    "filter_genre",
    "genre_operator",
    "released_from",
    "released_to",
    "filter_score_min",
    "filter_score_max",
    "sort",
    "order",
    "unified",
];

pub(crate) fn mdblist_media_info_url(
    provider: &str,
    media_type: &str,
    media_id: &str,
    append_to_response: Option<&str>,
) -> String {
    let mut params = Vec::new();
    if let Some(fields) = append_to_response.filter(|value| !value.is_empty()) {
        params.push(("append_to_response".to_string(), fields.to_string()));
    }
    build_url(
        &format!("/{provider}/{media_type}/{media_id}/"),
        &params,
    )
}

pub(crate) fn mdblist_watchprovider_links_url(
    provider: &str,
    media_type: &str,
    media_id: &str,
) -> String {
    build_url(
        &format!("/{provider}/{media_type}/{media_id}/watchprovider_links/"),
        &[],
    )
}

pub(crate) fn mdblist_media_info_batch_plan(
    provider: &str,
    media_type: &str,
    ids: &[String],
) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    plan(
        "POST",
        build_url(&format!("/{provider}/{media_type}/"), &[]),
        Some(json!({ "ids": ids })),
    )
}

pub(crate) fn mdblist_ratings_batch_plan(
    media_type: &str,
    return_rating: &str,
    provider: &str,
    ids: &[String],
) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    plan(
        "POST",
        build_url(&format!("/rating/{media_type}/{return_rating}"), &[]),
        Some(json!({ "provider": provider, "ids": ids })),
    )
}

pub(crate) fn mdblist_media_ratings_from_response_json(response_json: &str) -> Option<String> {
    let response: Value = serde_json::from_str(response_json).ok()?;
    let ratings = response.get("ratings")?.as_array()?;
    let mut normalized = serde_json::Map::new();
    for entry in ratings {
        let source = entry.get("source").and_then(Value::as_str)?;
        let value = entry
            .get("value")
            .filter(|value| !value.is_null())
            .or_else(|| entry.get("score").filter(|score| !score.is_null()));
        if let Some(value) = value {
            normalized.insert(source.to_string(), value.clone());
        }
    }
    serde_json::to_string(&Value::Object(normalized)).ok()
}

pub(crate) fn mdblist_search_url(media_type: &str, args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let params = extract_query(
        &args,
        &[
            "query",
            "quick_search",
            "limit_by_score",
            "sort_by_score",
            "year",
            "limit",
        ],
    );
    Some(build_url(&format!("/search/{media_type}"), &params))
}

pub(crate) fn mdblist_genres_url(anime: Option<bool>) -> String {
    let mut params = Vec::new();
    if let Some(anime) = anime {
        params.push(("anime".to_string(), if anime { "1" } else { "0" }.to_string()));
    }
    build_url("/genres", &params)
}

pub(crate) fn mdblist_catalog_url(media_type: &str, args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let mut params = extract_query(
        &args,
        &[
            "genre_mode",
            "country",
            "language",
            "score_min",
            "score_max",
            "released_from",
            "released_to",
            "year_min",
            "year_max",
            "runtime_min",
            "runtime_max",
            "sort",
            "sort_order",
            "limit",
            "cursor",
        ],
    );
    extract_repeated(&args, "genre", &mut params);
    Some(build_url(&format!("/catalog/{media_type}"), &params))
}

fn list_items_path(list_ref: &Value) -> Option<String> {
    if let Some(id) = list_ref.get("listId").and_then(Value::as_i64) {
        return Some(format!("/lists/{id}/items"));
    }
    if let Some(id) = list_ref.get("externalListId").and_then(Value::as_i64) {
        return Some(format!("/external/lists/{id}/items"));
    }
    if let Some(slug) = list_ref.get("officialSlug").and_then(Value::as_str) {
        return Some(format!("/lists/official/{slug}/items"));
    }
    if let Some(token) = list_ref.get("shareToken").and_then(Value::as_str) {
        return Some(format!("/lists/share/{token}/items"));
    }
    if let Some(section) = list_ref.get("recommendedSection").and_then(Value::as_str) {
        return Some(format!("/lists/recommended/{section}/items"));
    }
    let username = list_ref.get("username").and_then(Value::as_str)?;
    let list_name = list_ref.get("listName").and_then(Value::as_str)?;
    match list_ref.get("mediaType").and_then(Value::as_str) {
        Some(media_type) => Some(format!("/lists/{username}/{list_name}/items/{media_type}")),
        None => Some(format!("/lists/{username}/{list_name}/items")),
    }
}

pub(crate) fn mdblist_list_items_url(list_ref_json: &str, query_json: &str) -> Option<String> {
    let list_ref: Value = serde_json::from_str(list_ref_json).ok()?;
    let path = list_items_path(&list_ref)?;
    let query: Value = serde_json::from_str(query_json).unwrap_or(json!({}));
    Some(build_url(&path, &extract_query(&query, LIST_ITEMS_QUERY_KEYS)))
}

pub(crate) fn mdblist_list_item_to_meta_json(item_json: &str, media_type: &str) -> Option<String> {
    let item: Value = serde_json::from_str(item_json).ok()?;
    let ids = item.get("ids");
    let imdb_id = ids
        .and_then(|ids| ids.get("imdb"))
        .and_then(Value::as_str)
        .or_else(|| item.get("imdb_id").and_then(Value::as_str))
        .filter(|value| !value.is_empty());
    let tmdb_id = ids.and_then(|ids| ids.get("tmdb")).and_then(Value::as_i64);
    let id = if let Some(imdb_id) = imdb_id {
        imdb_id.to_string()
    } else if let Some(tmdb_id) = tmdb_id {
        format!("tmdb:{tmdb_id}")
    } else {
        return None;
    };
    let content_type = if media_type == "show" { "series" } else { "movie" };
    let title = item.get("title").and_then(Value::as_str).unwrap_or("");
    serde_json::to_string(&json!({
        "id": id,
        "type": content_type,
        "name": title,
        "releaseInfo": item.get("release_year"),
        "mdblistId": item.get("id"),
    }))
    .ok()
}

pub(crate) fn mdblist_list_items_response_to_metas_json(response_json: &str) -> Option<String> {
    let response: Value = serde_json::from_str(response_json).ok()?;
    let mut metas = Vec::new();
    for (key, media_type) in [("movies", "movie"), ("shows", "show")] {
        let Some(items) = response.get(key).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let item_json = serde_json::to_string(item).ok()?;
            if let Some(meta_json) = mdblist_list_item_to_meta_json(&item_json, media_type)
                && let Ok(meta) = serde_json::from_str::<Value>(&meta_json)
            {
                metas.push(meta);
            }
        }
    }
    serde_json::to_string(&metas).ok()
}

pub(crate) fn mdblist_list_info_url(listid: i64) -> String {
    build_url(&format!("/lists/{listid}"), &[])
}

pub(crate) fn mdblist_list_by_name_url(username: &str, list_name: &str) -> String {
    build_url(&format!("/lists/{username}/{list_name}"), &[])
}

pub(crate) fn mdblist_list_update_plan(
    listid: i64,
    name: Option<&str>,
    private: Option<bool>,
) -> Option<String> {
    let mut body = serde_json::Map::new();
    if let Some(name) = name {
        body.insert("name".to_string(), json!(name));
    }
    if let Some(private) = private {
        body.insert("private".to_string(), json!(private));
    }
    if body.is_empty() {
        return None;
    }
    plan(
        "PUT",
        build_url(&format!("/lists/{listid}"), &[]),
        Some(Value::Object(body)),
    )
}

pub(crate) fn mdblist_list_delete_plan(listid: i64) -> Option<String> {
    plan("DELETE", build_url(&format!("/lists/{listid}"), &[]), None)
}

pub(crate) fn mdblist_list_create_plan(name: &str, private: Option<bool>) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    let mut body = serde_json::Map::new();
    body.insert("name".to_string(), json!(name));
    if let Some(private) = private {
        body.insert("private".to_string(), json!(private));
    }
    plan(
        "POST",
        build_url("/lists/user/add", &[]),
        Some(Value::Object(body)),
    )
}

pub(crate) fn mdblist_list_changes_url(listid: i64) -> String {
    build_url(&format!("/lists/{listid}/changes"), &[])
}

pub(crate) fn mdblist_list_like_plan(listid: i64, liked: bool) -> Option<String> {
    let method = if liked { "PUT" } else { "DELETE" };
    plan(method, build_url(&format!("/lists/{listid}/like"), &[]), None)
}

fn ids_from_meta(meta: &Value) -> Option<Value> {
    let imdb = meta
        .get("imdbId")
        .or_else(|| meta.get("imdb"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let tmdb = meta.get("tmdbId").or_else(|| meta.get("tmdb")).and_then(Value::as_i64);
    let mut ids = serde_json::Map::new();
    if let Some(imdb) = imdb {
        ids.insert("imdb".to_string(), json!(imdb));
    }
    if let Some(tmdb) = tmdb {
        ids.insert("tmdb".to_string(), json!(tmdb));
    }
    if ids.is_empty() {
        return None;
    }
    Some(Value::Object(ids))
}

fn items_by_type_body(items_json: &str) -> Option<Value> {
    let items: Value = serde_json::from_str(items_json).ok()?;
    let mut movies = Vec::new();
    let mut shows = Vec::new();
    for item in items.as_array()? {
        let ids = ids_from_meta(item)?;
        let bucket = if item.get("type").and_then(Value::as_str) == Some("series") {
            &mut shows
        } else {
            &mut movies
        };
        let mut entry = serde_json::Map::new();
        entry.insert("ids".to_string(), ids);
        if let Some(rating) = item.get("rating") {
            entry.insert("rating".to_string(), rating.clone());
        }
        bucket.push(Value::Object(entry));
    }
    let mut body = serde_json::Map::new();
    if !movies.is_empty() {
        body.insert("movies".to_string(), Value::Array(movies));
    }
    if !shows.is_empty() {
        body.insert("shows".to_string(), Value::Array(shows));
    }
    if body.is_empty() {
        return None;
    }
    Some(Value::Object(body))
}

pub(crate) fn mdblist_list_items_mutate_plan(
    listid: i64,
    action: &str,
    items_json: &str,
) -> Option<String> {
    let body = items_by_type_body(items_json)?;
    plan(
        "POST",
        build_url(&format!("/lists/{listid}/items/{action}"), &[]),
        Some(body),
    )
}

pub(crate) fn mdblist_list_membership_url(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let params = extract_query(&args, &["mediatype", "tmdb", "imdb", "trakt", "tvdb"]);
    if !params.iter().any(|(key, _)| key == "mediatype") {
        return None;
    }
    Some(build_url("/lists/user/membership", &params))
}

pub(crate) fn mdblist_lists_search_url(query: &str) -> Option<String> {
    if query.trim().is_empty() {
        return None;
    }
    Some(build_url(
        "/lists/search",
        &[("query".to_string(), query.to_string())],
    ))
}

pub(crate) fn mdblist_lists_curated_url(args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
    build_url(
        "/lists/curated",
        &extract_query(&args, &["limit", "offset", "append_to_response"]),
    )
}

pub(crate) fn mdblist_lists_top_url(args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
    build_url(
        "/lists/top",
        &extract_query(&args, &["limit", "offset", "append_to_response"]),
    )
}

pub(crate) fn mdblist_lists_liked_url(args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
    build_url(
        "/lists/liked",
        &extract_query(&args, &["limit", "offset", "append_to_response"]),
    )
}

pub(crate) fn mdblist_lists_official_url() -> String {
    build_url("/lists/official", &[])
}

pub(crate) fn mdblist_lists_recommended_url(section: Option<&str>) -> String {
    match section.filter(|value| !value.is_empty()) {
        Some(section) => build_url(&format!("/lists/recommended/{section}"), &[]),
        None => build_url("/lists/recommended", &[]),
    }
}

pub(crate) fn mdblist_lists_user_url(user_ref: Option<&str>, args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
    let params = extract_query(&args, &["sort", "unified", "append_to_response"]);
    match user_ref.filter(|value| !value.is_empty()) {
        Some(user_ref) => build_url(&format!("/lists/user/{user_ref}"), &params),
        None => build_url("/lists/user", &params),
    }
}

const WATCHLIST_QUERY_KEYS: &[&str] = &[
    "cursor",
    "limit",
    "offset",
    "append_to_response",
    "extended",
    "sort",
    "order",
];

pub(crate) fn mdblist_watchlist_items_url(media_type: Option<&str>, args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
    let params = extract_query(&args, WATCHLIST_QUERY_KEYS);
    match media_type.filter(|value| !value.is_empty()) {
        Some(media_type) => build_url(&format!("/watchlist/items/{media_type}"), &params),
        None => build_url("/watchlist/items", &params),
    }
}

pub(crate) fn mdblist_watchlist_mutate_plan(action: &str, items_json: &str) -> Option<String> {
    let body = items_by_type_body(items_json)?;
    plan(
        "POST",
        build_url(&format!("/watchlist/items/{action}"), &[]),
        Some(body),
    )
}

const SYNC_LIST_QUERY_KEYS: &[&str] = &[
    "cursor",
    "limit",
    "offset",
    "since",
    "page",
    "mediatype",
    "sort",
    "order",
    "unified",
    "append_to_response",
    "extended",
];

pub(crate) fn mdblist_sync_get_url(category: &str, args_json: &str) -> Option<String> {
    let path = match category {
        "watched" | "ratings" | "collection" | "dropped" | "journal" | "last_activities"
        | "now-playing" | "playback" => format!("/sync/{category}"),
        _ => return None,
    };
    let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
    let keys: &[&str] = match category {
        "journal" => &["since", "limit", "cursor"],
        "last_activities" | "now-playing" | "playback" => &[],
        _ => SYNC_LIST_QUERY_KEYS,
    };
    Some(build_url(&path, &extract_query(&args, keys)))
}

pub(crate) fn mdblist_sync_mutate_plan(
    category: &str,
    remove: bool,
    items_json: &str,
) -> Option<String> {
    if !["watched", "ratings", "collection", "dropped"].contains(&category) {
        return None;
    }
    let body = items_by_type_body(items_json)?;
    let path = if remove {
        format!("/sync/{category}/remove")
    } else {
        format!("/sync/{category}")
    };
    plan("POST", build_url(&path, &[]), Some(body))
}

const UPNEXT_QUERY_KEYS: &[&str] = &["limit", "offset", "hide_unreleased", "days"];

pub(crate) fn mdblist_upnext_url(section: Option<&str>, args_json: &str) -> Option<String> {
    let path = match section {
        None => "/upnext".to_string(),
        Some("upcoming") => "/upnext/upcoming".to_string(),
        Some("watchlist") => "/upnext/watchlist".to_string(),
        Some(_) => return None,
    };
    let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
    Some(build_url(&path, &extract_query(&args, UPNEXT_QUERY_KEYS)))
}

// Confirmed against a real account: the body nests under "movie" or "show"
// (Trakt-shaped), not a flat {ids, type}. Season/episode live inside the
// "show" object itself, not a separate "episode" object.
fn scrobble_target_body(args: &Value) -> Option<serde_json::Map<String, Value>> {
    let ids = args.get("ids").filter(|value| value.is_object())?.clone();
    let mut body = serde_json::Map::new();
    if args.get("isEpisode").and_then(Value::as_bool) == Some(true) {
        let mut show = serde_json::Map::new();
        show.insert("ids".to_string(), ids);
        if let Some(season) = args.get("season") {
            show.insert("season".to_string(), season.clone());
        }
        if let Some(episode) = args.get("episode") {
            show.insert("episode".to_string(), episode.clone());
        }
        body.insert("show".to_string(), Value::Object(show));
    } else {
        body.insert("movie".to_string(), json!({ "ids": ids }));
    }
    Some(body)
}

pub(crate) fn mdblist_scrobble_plan(action: &str, args_json: &str) -> Option<String> {
    let path = match action {
        "start" => "/scrobble/start",
        "pause" => "/scrobble/pause",
        "stop" => "/scrobble/stop",
        "clear" => "/scrobble/clear",
        _ => return None,
    };
    let args: Value = serde_json::from_str(args_json).ok()?;
    let mut body = scrobble_target_body(&args)?;
    if let Some(progress) = args.get("progress") {
        body.insert("progress".to_string(), progress.clone());
    }
    plan("POST", build_url(path, &[]), Some(Value::Object(body)))
}

pub(crate) fn mdblist_checkin_plan(method: &str, args_json: &str) -> Option<String> {
    let body = if method == "POST" {
        let args: Value = serde_json::from_str(args_json).ok()?;
        Some(Value::Object(scrobble_target_body(&args)?))
    } else {
        None
    };
    match method {
        "GET" | "POST" | "DELETE" => plan(method, build_url("/checkin", &[]), body),
        _ => None,
    }
}

pub(crate) fn mdblist_discussion_url(provider: &str, target_type: &str, target_id: i64) -> String {
    build_url(
        &format!("/discussion/{provider}/{target_type}/{target_id}"),
        &[],
    )
}

pub(crate) fn mdblist_discussion_summary_url(
    provider: &str,
    target_type: &str,
    target_id: i64,
) -> String {
    build_url(
        &format!("/discussion/{provider}/{target_type}/{target_id}/summary"),
        &[],
    )
}

pub(crate) fn mdblist_discussion_create_plan(
    provider: &str,
    target_type: &str,
    target_id: i64,
    comment: &str,
) -> Option<String> {
    if comment.trim().is_empty() {
        return None;
    }
    plan(
        "POST",
        build_url(&format!("/discussion/{provider}/{target_type}/{target_id}"), &[]),
        Some(json!({ "content": comment })),
    )
}

pub(crate) fn mdblist_discussion_hot_url() -> String {
    build_url("/discussion/hot", &[])
}

pub(crate) fn mdblist_discussion_replies_url(comment_id: i64, args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
    build_url(
        &format!("/discussion/comments/{comment_id}/replies"),
        &extract_query(&args, &["limit", "offset"]),
    )
}

pub(crate) fn mdblist_discussion_reply_create_plan(comment_id: i64, comment: &str) -> Option<String> {
    if comment.trim().is_empty() {
        return None;
    }
    plan(
        "POST",
        build_url(&format!("/discussion/comments/{comment_id}/replies"), &[]),
        Some(json!({ "content": comment })),
    )
}

pub(crate) fn mdblist_discussion_comment_update_plan(comment_id: i64, comment: &str) -> Option<String> {
    if comment.trim().is_empty() {
        return None;
    }
    plan(
        "PATCH",
        build_url(&format!("/discussion/comments/{comment_id}"), &[]),
        Some(json!({ "content": comment })),
    )
}

pub(crate) fn mdblist_discussion_comment_delete_plan(comment_id: i64) -> Option<String> {
    plan(
        "DELETE",
        build_url(&format!("/discussion/comments/{comment_id}"), &[]),
        None,
    )
}

pub(crate) fn mdblist_discussion_comment_like_plan(comment_id: i64) -> Option<String> {
    plan(
        "POST",
        build_url(&format!("/discussion/comments/{comment_id}/like"), &[]),
        None,
    )
}

pub(crate) fn mdblist_discussion_reply_update_plan(reply_id: i64, comment: &str) -> Option<String> {
    if comment.trim().is_empty() {
        return None;
    }
    plan(
        "PATCH",
        build_url(&format!("/discussion/replies/{reply_id}"), &[]),
        Some(json!({ "content": comment })),
    )
}

pub(crate) fn mdblist_discussion_reply_delete_plan(reply_id: i64) -> Option<String> {
    plan(
        "DELETE",
        build_url(&format!("/discussion/replies/{reply_id}"), &[]),
        None,
    )
}

pub(crate) fn mdblist_discussion_reply_like_plan(reply_id: i64) -> Option<String> {
    plan(
        "POST",
        build_url(&format!("/discussion/replies/{reply_id}/like"), &[]),
        None,
    )
}

pub(crate) fn mdblist_user_url() -> String {
    build_url("/user", &[])
}

pub(crate) fn mdblist_user_stats_url() -> String {
    build_url("/user/stats", &[])
}

pub(crate) fn mdblist_public_user_url(username: &str) -> String {
    build_url(&format!("/users/{username}"), &[])
}

pub(crate) fn mdblist_user_follow_plan(username: &str, follow: bool) -> Option<String> {
    let method = if follow { "POST" } else { "DELETE" };
    plan(
        method,
        build_url(&format!("/users/{username}/follow"), &[]),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_media_info_and_batch_requests() {
        assert_eq!(
            mdblist_media_info_url("imdb", "movie", "tt0111161", Some("ratings")),
            "https://api.mdblist.com/imdb/movie/tt0111161/?append_to_response=ratings"
        );
        let plan: Value = serde_json::from_str(
            &mdblist_media_info_batch_plan("imdb", "movie", &["tt1".to_string(), "tt2".to_string()])
                .unwrap(),
        )
        .unwrap();
        assert_eq!(plan["method"], "POST");
        assert_eq!(plan["url"], "https://api.mdblist.com/imdb/movie/");
        assert_eq!(plan["body"]["ids"], json!(["tt1", "tt2"]));
        assert!(mdblist_media_info_batch_plan("imdb", "movie", &[]).is_none());
    }

    #[test]
    fn normalizes_ratings_response_into_a_flat_map() {
        let response = json!({
            "ratings": [
                { "source": "imdb", "value": 9.3 },
                { "source": "tmdb", "score": 88 },
                { "source": "myanimelist", "value": null, "score": null },
                { "source": "metacriticuser", "value": null, "score": 75 }
            ]
        })
        .to_string();
        let normalized: Value =
            serde_json::from_str(&mdblist_media_ratings_from_response_json(&response).unwrap())
                .unwrap();
        assert_eq!(normalized["imdb"], 9.3);
        assert_eq!(normalized["tmdb"], 88);
        assert!(normalized.get("myanimelist").is_none());
        assert_eq!(normalized["metacriticuser"], 75);
    }

    #[test]
    fn builds_catalog_url_with_repeated_genre_params() {
        let url = mdblist_catalog_url(
            "movie",
            r#"{"genre": ["action", "comedy"], "year_min": 2020, "sort": "score"}"#,
        )
        .unwrap();
        assert!(url.starts_with("https://api.mdblist.com/catalog/movie?"));
        assert!(url.contains("genre=action"));
        assert!(url.contains("genre=comedy"));
        assert!(url.contains("year_min=2020"));
        assert!(url.contains("sort=score"));
    }

    #[test]
    fn resolves_list_items_url_variants() {
        assert_eq!(
            mdblist_list_items_url(r#"{"listId": 42}"#, "{}").unwrap(),
            "https://api.mdblist.com/lists/42/items"
        );
        assert_eq!(
            mdblist_list_items_url(
                r#"{"username": "alice", "listName": "top-picks"}"#,
                r#"{"limit": 20}"#
            )
            .unwrap(),
            "https://api.mdblist.com/lists/alice/top-picks/items?limit=20"
        );
        assert_eq!(
            mdblist_list_items_url(r#"{"officialSlug": "oscar-winners"}"#, "{}").unwrap(),
            "https://api.mdblist.com/lists/official/oscar-winners/items"
        );
    }

    #[test]
    fn converts_list_items_response_to_metas() {
        let response = json!({
            "movies": [{ "id": 1, "title": "Movie A", "ids": { "imdb": "tt1" }, "release_year": 2020 }],
            "shows": [{ "id": 2, "title": "Show B", "ids": { "tmdb": 55 } }]
        })
        .to_string();
        let metas: Value =
            serde_json::from_str(&mdblist_list_items_response_to_metas_json(&response).unwrap())
                .unwrap();
        let metas = metas.as_array().unwrap();
        assert_eq!(metas.len(), 2);
        assert_eq!(metas[0]["id"], "tt1");
        assert_eq!(metas[0]["type"], "movie");
        assert_eq!(metas[1]["id"], "tmdb:55");
        assert_eq!(metas[1]["type"], "series");
    }

    #[test]
    fn builds_list_mutation_plans() {
        let items = json!([
            { "type": "movie", "imdbId": "tt1" },
            { "type": "series", "tmdbId": 99 }
        ])
        .to_string();
        let plan: Value =
            serde_json::from_str(&mdblist_list_items_mutate_plan(7, "add", &items).unwrap())
                .unwrap();
        assert_eq!(plan["url"], "https://api.mdblist.com/lists/7/items/add");
        assert_eq!(plan["body"]["movies"][0]["ids"]["imdb"], "tt1");
        assert_eq!(plan["body"]["shows"][0]["ids"]["tmdb"], 99);
    }

    #[test]
    fn builds_sync_and_watchlist_mutation_plans() {
        let items = json!([{ "type": "movie", "imdbId": "tt1" }]).to_string();
        let add: Value =
            serde_json::from_str(&mdblist_sync_mutate_plan("watched", false, &items).unwrap())
                .unwrap();
        assert_eq!(add["url"], "https://api.mdblist.com/sync/watched");
        let remove: Value =
            serde_json::from_str(&mdblist_sync_mutate_plan("watched", true, &items).unwrap())
                .unwrap();
        assert_eq!(remove["url"], "https://api.mdblist.com/sync/watched/remove");
        assert!(mdblist_sync_mutate_plan("bogus", false, &items).is_none());

        let watchlist_items = json!([{ "type": "movie", "imdbId": "tt1" }]).to_string();
        let watchlist: Value =
            serde_json::from_str(&mdblist_watchlist_mutate_plan("add", &watchlist_items).unwrap())
                .unwrap();
        assert_eq!(watchlist["url"], "https://api.mdblist.com/watchlist/items/add");

        let rated_items = json!([{ "type": "movie", "imdbId": "tt1", "rating": 8 }]).to_string();
        let rate: Value =
            serde_json::from_str(&mdblist_sync_mutate_plan("ratings", false, &rated_items).unwrap())
                .unwrap();
        assert_eq!(rate["body"]["movies"][0]["rating"], 8);
    }

    #[test]
    fn builds_scrobble_and_checkin_plans() {
        let movie_args = json!({ "ids": {"imdb": "tt1"}, "progress": 42.5 }).to_string();
        let start: Value =
            serde_json::from_str(&mdblist_scrobble_plan("start", &movie_args).unwrap()).unwrap();
        assert_eq!(start["url"], "https://api.mdblist.com/scrobble/start");
        assert_eq!(start["body"]["progress"], 42.5);
        assert_eq!(start["body"]["movie"]["ids"]["imdb"], "tt1");
        assert!(mdblist_scrobble_plan("bogus", &movie_args).is_none());

        let episode_args = json!({
            "ids": {"imdb": "tt2"},
            "isEpisode": true,
            "season": 1,
            "episode": 3,
            "progress": 10
        })
        .to_string();
        let episode_start: Value =
            serde_json::from_str(&mdblist_scrobble_plan("start", &episode_args).unwrap()).unwrap();
        assert_eq!(episode_start["body"]["show"]["ids"]["imdb"], "tt2");
        assert_eq!(episode_start["body"]["show"]["season"], 1);
        assert_eq!(episode_start["body"]["show"]["episode"], 3);
        assert!(episode_start["body"].get("movie").is_none());

        let checkin_start: Value =
            serde_json::from_str(&mdblist_checkin_plan("POST", &movie_args).unwrap()).unwrap();
        assert_eq!(checkin_start["method"], "POST");
        assert_eq!(checkin_start["body"]["movie"]["ids"]["imdb"], "tt1");
        let checkin_stop: Value =
            serde_json::from_str(&mdblist_checkin_plan("DELETE", "{}").unwrap()).unwrap();
        assert_eq!(checkin_stop["body"], Value::Null);
    }

    #[test]
    fn builds_discussion_plans() {
        assert_eq!(
            mdblist_discussion_url("tmdb", "movie", 1),
            "https://api.mdblist.com/discussion/tmdb/movie/1"
        );
        let create: Value = serde_json::from_str(
            &mdblist_discussion_create_plan("tmdb", "movie", 1, "great film").unwrap(),
        )
        .unwrap();
        assert_eq!(create["body"]["content"], "great film");
        assert!(mdblist_discussion_create_plan("tmdb", "movie", 1, "  ").is_none());
    }

    #[test]
    fn device_poll_outcome_reads_oauth_error_field() {
        assert_eq!(
            mdblist_device_poll_outcome(r#"{"error":"authorization_pending"}"#),
            "pending"
        );
        assert_eq!(
            mdblist_device_poll_outcome(r#"{"error":"expired_token"}"#),
            "expired"
        );
        assert_eq!(
            mdblist_device_poll_outcome(r#"{"access_token":"tok"}"#),
            "success"
        );
    }
}
