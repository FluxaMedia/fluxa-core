use serde_json::{json, Map, Value};

const RESOLVED_LOW_RATIO: f64 = 0.005;
const RESOLVED_HIGH_RATIO: f64 = 0.995;
const RESOLVED_MAX_POSITION_MS: f64 = 1000.0;
const AVATAR_STORAGE_BASE: &str =
    "https://dpyhjjcoabcglfmgecug.supabase.co/storage/v1/object/public/avatars/";

fn parse(args_json: &str) -> Option<Value> {
    serde_json::from_str(args_json).ok()
}

fn str_field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(Value::as_str)
}

fn iso_from_ms(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}

fn safe_id_part(value: &str) -> String {
    let cleaned: String = value
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "user".to_string()
    } else {
        cleaned
    }
}

fn avatar_url_for(profile: &Value, avatar_catalog: &[Value]) -> Option<String> {
    if let Some(url) = str_field(profile, "avatar_url").filter(|s| !s.is_empty()) {
        return Some(url.to_string());
    }
    let avatar_id = profile.get("avatar_id").filter(|v| !v.is_null())?;
    let entry = avatar_catalog.iter().find(|a| a.get("id") == Some(avatar_id))?;
    let storage_path = str_field(entry, "storage_path").filter(|s| !s.is_empty())?;
    Some(format!("{AVATAR_STORAGE_BASE}{storage_path}"))
}

pub(crate) fn build_local_profiles_json(args_json: &str) -> Option<String> {
    let args = parse(args_json)?;
    let session = args.get("sessionProfile")?;
    let remote_profiles = args
        .get("nuvioProfiles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let avatar_catalog = args
        .get("avatarCatalog")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let existing_profiles = args
        .get("existingProfiles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let session_user_id = session.get("nuvioUserId").cloned().unwrap_or(Value::Null);
    let remote_profiles = if remote_profiles.is_empty() {
        vec![json!({
            "profile_index": 1,
            "name": str_field(session, "name").filter(|s| !s.is_empty()).unwrap_or("Primary"),
            "avatar_color_hex": Value::Null,
            "avatar_id": Value::Null,
            "avatar_url": Value::Null,
        })]
    } else {
        remote_profiles
    };

    let mut by_nuvio_index: Map<String, Value> = Map::new();
    for p in &existing_profiles {
        let matches_user = !session_user_id.is_null() && p.get("nuvioUserId") == Some(&session_user_id);
        if let (true, Some(index)) = (matches_user, p.get("nuvioProfileIndex").and_then(Value::as_i64)) {
            by_nuvio_index.insert(index.to_string(), p.clone());
        }
    }

    let fallback_id_part = str_field(session, "nuvioUserId")
        .or_else(|| str_field(session, "nuvioEmail"))
        .or_else(|| str_field(session, "email"))
        .unwrap_or("user");

    let mut imported_ids: Vec<Value> = Vec::new();
    let mut imported: Vec<Value> = Vec::new();
    for remote in &remote_profiles {
        let index = remote.get("profile_index").and_then(Value::as_i64).unwrap_or(1);
        let existing = by_nuvio_index.get(&index.to_string());
        let mut out = existing
            .and_then(|e| e.as_object().cloned())
            .unwrap_or_default();
        let id = existing
            .and_then(|e| str_field(e, "id"))
            .map(str::to_string)
            .unwrap_or_else(|| format!("nuvio_{}_{index}", safe_id_part(fallback_id_part)));
        imported_ids.push(Value::String(id.clone()));
        out.insert("id".into(), Value::String(id));

        let name = str_field(remote, "name")
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| existing.and_then(|e| str_field(e, "name")).map(str::to_string))
            .unwrap_or_else(|| format!("Profile {index}"));
        out.insert("name".into(), Value::String(name));

        if let Some(url) = avatar_url_for(remote, &avatar_catalog) {
            out.insert("avatarUrl".into(), Value::String(url));
        }
        if let Some(color) = remote.get("avatar_color_hex").filter(|v| !v.is_null()) {
            out.insert("color".into(), color.clone());
        }
        for (dst, src) in [
            ("email", "email"),
            ("nuvioAccessToken", "nuvioAccessToken"),
            ("nuvioRefreshToken", "nuvioRefreshToken"),
            ("nuvioTokenExpiresAt", "nuvioTokenExpiresAt"),
            ("nuvioUserId", "nuvioUserId"),
            ("nuvioEmail", "nuvioEmail"),
        ] {
            match session.get(src) {
                Some(v) if !v.is_null() => {
                    out.insert(dst.into(), v.clone());
                }
                _ => {
                    out.remove(dst);
                }
            }
        }
        out.insert("nuvioProfileIndex".into(), json!(index));
        imported.push(Value::Object(out));
    }

    let mut result: Vec<Value> = existing_profiles
        .into_iter()
        .filter(|p| {
            p.get("id")
                .map(|id| !imported_ids.contains(id))
                .unwrap_or(true)
        })
        .collect();
    result.extend(imported);
    Some(Value::Array(result).to_string())
}

pub(crate) fn library_to_watchlist_json(args_json: &str) -> Option<String> {
    let args = parse(args_json)?;
    let library = args.get("library")?.as_array()?.clone();
    let watchlist: Vec<Value> = library
        .iter()
        .map(|item| {
            let mut out = Map::new();
            out.insert("id".into(), item.get("content_id").cloned().unwrap_or(Value::Null));
            out.insert("name".into(), item.get("name").cloned().unwrap_or(Value::Null));
            out.insert("type".into(), item.get("content_type").cloned().unwrap_or(Value::Null));
            for (dst, src) in [
                ("poster", "poster"),
                ("background", "background"),
                ("description", "description"),
                ("releaseInfo", "release_info"),
                ("imdbRating", "imdb_rating"),
            ] {
                if let Some(v) = item.get(src).filter(|v| !v.is_null()) {
                    out.insert(dst.into(), v.clone());
                }
            }
            if let Some(genres) = item.get("genres").and_then(Value::as_array) {
                if !genres.is_empty() {
                    out.insert("genres".into(), Value::Array(genres.clone()));
                }
            }
            out.insert("inWatchlist".into(), Value::Bool(true));
            Value::Object(out)
        })
        .collect();
    Some(Value::Array(watchlist).to_string())
}

pub(crate) fn progress_meta_needs_json(args_json: &str) -> Option<String> {
    let args = parse(args_json)?;
    let watch_progress = args.get("watchProgress")?.as_array()?.clone();
    let library = args
        .get("library")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let library_ids: Vec<&Value> = library.iter().filter_map(|i| i.get("content_id")).collect();

    let needs: Vec<Value> = watch_progress
        .iter()
        .filter(|e| {
            let is_series = str_field(e, "content_type") == Some("series");
            let in_library = e
                .get("content_id")
                .map(|id| library_ids.contains(&id))
                .unwrap_or(false);
            is_series || !in_library
        })
        .map(|e| {
            json!({
                "contentId": e.get("content_id").cloned().unwrap_or(Value::Null),
                "contentType": e.get("content_type").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    Some(Value::Array(needs).to_string())
}

fn progress_entry(entry: &Value, lib_item: Option<&Value>, addon_meta: Option<&Value>) -> Value {
    let position = entry.get("position").and_then(Value::as_f64).unwrap_or(0.0);
    let duration = entry.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
    let ratio = if duration > 0.0 { position / duration } else { 0.0 };
    let is_resolved_up_next = if duration <= 0.0 {
        position <= RESOLVED_MAX_POSITION_MS
    } else {
        ratio < RESOLVED_LOW_RATIO || ratio >= RESOLVED_HIGH_RATIO
    };

    let season = entry.get("season").filter(|v| !v.is_null());
    let episode = entry.get("episode").filter(|v| !v.is_null());
    let num_eq = |a: Option<&Value>, b: Option<&Value>| match (a.and_then(Value::as_f64), b.and_then(Value::as_f64)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    let ep_meta = match (season, episode, addon_meta.and_then(|m| m.get("videos")).and_then(Value::as_array)) {
        (Some(s), Some(e), Some(videos)) => videos
            .iter()
            .find(|v| num_eq(v.get("season"), Some(s)) && num_eq(v.get("episode"), Some(e))),
        _ => None,
    };

    let pick = |field: &str| -> Value {
        lib_item
            .and_then(|i| i.get(field))
            .filter(|v| !v.is_null())
            .or_else(|| addon_meta.and_then(|m| m.get(field)).filter(|v| !v.is_null()))
            .cloned()
            .unwrap_or(Value::Null)
    };

    let mut out = Map::new();
    let mut meta = Map::new();
    meta.insert("id".into(), entry.get("content_id").cloned().unwrap_or(Value::Null));
    meta.insert("type".into(), entry.get("content_type").cloned().unwrap_or(Value::Null));
    meta.insert("name".into(), pick("name"));
    for field in ["poster", "background"] {
        let v = pick(field);
        if !v.is_null() {
            meta.insert(field.into(), v);
        }
    }
    out.insert("meta".into(), Value::Object(meta));
    out.insert("timeOffset".into(), json!((position / 1000.0).round() as i64));
    out.insert("duration".into(), json!((duration / 1000.0).round() as i64));
    out.insert("lastVideoId".into(), entry.get("video_id").cloned().unwrap_or(Value::Null));
    if let Some(s) = season {
        out.insert("lastEpisodeSeason".into(), s.clone());
    }
    if let Some(e) = episode {
        out.insert("lastEpisodeNumber".into(), e.clone());
    }
    if let Some(ep) = ep_meta {
        if let Some(title) = str_field(ep, "title").or_else(|| str_field(ep, "name")) {
            out.insert("lastEpisodeName".into(), Value::String(title.to_string()));
        }
        if let Some(thumb) = str_field(ep, "thumbnail") {
            out.insert("lastEpisodeThumbnail".into(), Value::String(thumb.to_string()));
        }
    }
    if is_resolved_up_next {
        out.insert("continueWatchingBadge".into(), Value::String("upNext".into()));
        out.insert("continueWatchingEpisodeResolved".into(), Value::Bool(true));
    }
    let last_watched = entry.get("last_watched").and_then(Value::as_i64).unwrap_or(0);
    out.insert("savedAt".into(), Value::String(iso_from_ms(last_watched)));
    out.insert("source".into(), Value::String("nuvio".into()));
    Value::Object(out)
}

pub(crate) fn import_merge_plan_json(args_json: &str) -> Option<String> {
    let args = parse(args_json)?;
    let mut progress = args
        .get("progress")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut watched = args
        .get("watched")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let library = args
        .get("library")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let addon_metas = args
        .get("addonMetas")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut lib_by_id: Map<String, Value> = Map::new();
    for item in library {
        if let Some(id) = str_field(&item, "content_id") {
            lib_by_id.insert(id.to_string(), item.clone());
        }
    }

    let mut active_remote_ids: Vec<String> = Vec::new();
    if let Some(watch_progress) = args.get("watchProgress").and_then(Value::as_array) {
        let mut sorted = watch_progress.clone();
        sorted.sort_by_key(|e| e.get("last_watched").and_then(Value::as_i64).unwrap_or(0));
        for entry in &sorted {
            let Some(content_id) = str_field(entry, "content_id") else {
                continue;
            };
            progress.insert(
                content_id.to_string(),
                progress_entry(entry, lib_by_id.get(content_id), addon_metas.get(content_id)),
            );
            if let Some(video_id) = str_field(entry, "video_id") {
                active_remote_ids.push(video_id.to_string());
            }
            if let (Some(s), Some(e)) = (
                entry.get("season").and_then(Value::as_i64),
                entry.get("episode").and_then(Value::as_i64),
            ) {
                active_remote_ids.push(format!("{content_id}:{s}:{e}"));
            }
        }
    }

    if let Some(watch_history) = args.get("watchHistory").and_then(Value::as_array) {
        for item in watch_history {
            let Some(content_id) = str_field(item, "content_id") else {
                continue;
            };
            if str_field(item, "content_type") == Some("movie") {
                watched.insert(content_id.to_string(), Value::Bool(true));
            } else if let (Some(s), Some(e)) = (
                item.get("season").and_then(Value::as_i64),
                item.get("episode").and_then(Value::as_i64),
            ) {
                watched.insert(format!("{content_id}:{s}:{e}"), Value::Bool(true));
            }
        }
        for id in &active_remote_ids {
            watched.remove(id);
        }
    }

    let is_watched = |watched: &Map<String, Value>, key: &str| {
        watched.get(key).and_then(Value::as_bool).unwrap_or(false)
    };
    let mut to_remove: Vec<String> = Vec::new();
    for (content_id, entry) in &progress {
        let video_watched = str_field(entry, "lastVideoId")
            .map(|id| is_watched(&watched, id))
            .unwrap_or(false);
        let episode_watched = match (
            entry.get("lastEpisodeSeason").and_then(Value::as_i64),
            entry.get("lastEpisodeNumber").and_then(Value::as_i64),
        ) {
            (Some(s), Some(e)) => is_watched(&watched, &format!("{content_id}:{s}:{e}")),
            _ => false,
        };
        if video_watched || episode_watched {
            to_remove.push(content_id.clone());
        }
    }
    for id in to_remove {
        progress.remove(&id);
    }

    Some(
        json!({
            "progress": progress,
            "watched": watched,
        })
        .to_string(),
    )
}

fn map_catalog_source(source: &Value) -> Option<Value> {
    let addon_id = str_field(source, "addonId").unwrap_or("");
    Some(json!({
        "addonId": addon_id,
        "catalogId": str_field(source, "catalogId").unwrap_or(""),
        "type": str_field(source, "type").unwrap_or("movie"),
        "genre": str_field(source, "genre"),
    }))
}

fn map_folder_source(source: &Value) -> Option<Value> {
    let provider = str_field(source, "provider").unwrap_or("addon").to_lowercase();
    let mut out = source.as_object().cloned().unwrap_or_default();
    match provider.as_str() {
        "trakt" => {
            source.get("traktListId").and_then(Value::as_i64)?;
            out.insert("provider".into(), Value::String("trakt".into()));
            for field in ["title", "mediaType", "sortBy", "sortHow"] {
                if !source.get(field).map(Value::is_string).unwrap_or(false) {
                    out.remove(field);
                }
            }
        }
        "tmdb" => {
            str_field(source, "tmdbSourceType")?;
            out.insert("provider".into(), Value::String("tmdb".into()));
            for field in ["title", "mediaType", "sortBy", "sortHow"] {
                if !source.get(field).map(Value::is_string).unwrap_or(false) {
                    out.remove(field);
                }
            }
            if !source.get("tmdbId").map(|v| v.is_i64() || v.is_u64()).unwrap_or(false) {
                out.remove("tmdbId");
            }
            let filters_ok = source
                .get("filters")
                .map(|v| v.is_object())
                .unwrap_or(false);
            if !filters_ok {
                out.remove("filters");
            }
        }
        "addon" => {
            str_field(source, "addonId")?;
            str_field(source, "type")?;
            str_field(source, "catalogId")?;
            out.insert("provider".into(), Value::String("addon".into()));
            if !source.get("genre").map(Value::is_string).unwrap_or(false) {
                out.remove("genre");
            }
        }
        _ => return None,
    }
    Some(Value::Object(out))
}

fn normalize_tile_shape(value: Option<&str>) -> String {
    let raw = value.unwrap_or("poster").to_lowercase();
    if raw == "landscape" {
        "wide".to_string()
    } else {
        raw
    }
}

fn map_folder(folder: &Value) -> Value {
    let mut out = folder.as_object().cloned().unwrap_or_default();
    out.insert(
        "id".into(),
        Value::String(
            folder
                .get("id")
                .map(value_to_display_string)
                .unwrap_or_default(),
        ),
    );
    out.insert(
        "title".into(),
        Value::String(
            folder
                .get("title")
                .map(value_to_display_string)
                .unwrap_or_default(),
        ),
    );
    for field in ["coverImageUrl", "coverEmoji", "focusGifUrl", "titleLogoUrl", "heroBackdropUrl", "heroVideoUrl"] {
        if !folder.get(field).map(Value::is_string).unwrap_or(false) {
            out.remove(field);
        }
    }
    out.insert(
        "focusGifEnabled".into(),
        Value::Bool(folder.get("focusGifEnabled") != Some(&Value::Bool(false))),
    );
    out.insert(
        "shape".into(),
        Value::String(normalize_tile_shape(str_field(folder, "tileShape"))),
    );
    out.insert(
        "hideTitle".into(),
        Value::Bool(folder.get("hideTitle").and_then(Value::as_bool).unwrap_or(false)),
    );

    let sources = folder
        .get("sources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let catalog_sources: Vec<Value> = if !sources.is_empty() {
        sources
            .iter()
            .filter(|s| str_field(s, "provider").unwrap_or("addon").to_lowercase() == "addon")
            .filter_map(map_catalog_source)
            .collect()
    } else {
        folder
            .get("catalogSources")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(map_catalog_source).collect())
            .unwrap_or_default()
    };
    out.insert("catalogSources".into(), Value::Array(catalog_sources));
    out.insert(
        "sources".into(),
        Value::Array(sources.iter().filter_map(map_folder_source).collect()),
    );
    Value::Object(out)
}

fn value_to_display_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn map_collections_json(args_json: &str) -> Option<String> {
    let args = parse(args_json)?;
    let collections = args.get("collections")?.as_array()?.clone();
    let mapped: Vec<Value> = collections
        .iter()
        .map(|c| {
            let mut out = c.as_object().cloned().unwrap_or_default();
            out.insert(
                "id".into(),
                Value::String(c.get("id").map(value_to_display_string).unwrap_or_default()),
            );
            out.insert(
                "title".into(),
                Value::String(c.get("title").map(value_to_display_string).unwrap_or_default()),
            );
            match c.get("backdropImageUrl").filter(|v| v.is_string()) {
                Some(url) => {
                    out.insert("imageUrl".into(), url.clone());
                    out.insert("backdropImageUrl".into(), url.clone());
                }
                None => {
                    out.remove("imageUrl");
                    out.remove("backdropImageUrl");
                }
            }
            out.insert("showOnHome".into(), Value::Bool(true));
            out.insert(
                "viewMode".into(),
                c.get("viewMode")
                    .filter(|v| v.is_string())
                    .cloned()
                    .unwrap_or_else(|| Value::String("ROWS".into())),
            );
            for field in ["showAllTab", "pinToTop"] {
                out.insert(
                    field.into(),
                    Value::Bool(c.get(field).and_then(Value::as_bool).unwrap_or(false)),
                );
            }
            let folders = c
                .get("folders")
                .and_then(Value::as_array)
                .map(|list| list.iter().map(map_folder).collect())
                .unwrap_or_default();
            out.insert("folders".into(), Value::Array(folders));
            Value::Object(out)
        })
        .collect();
    Some(Value::Array(mapped).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merge(args: Value) -> Value {
        serde_json::from_str(&import_merge_plan_json(&args.to_string()).unwrap()).unwrap()
    }

    #[test]
    fn watched_episode_removes_its_progress_entry() {
        let result = merge(json!({
            "progress": {},
            "watched": {},
            "library": [],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "series", "video_id": "tt1:1:2",
                "position": 500_000, "duration": 1_000_000,
                "season": 1, "episode": 2, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": [
                { "content_id": "tt1", "content_type": "series", "season": 1, "episode": 3, "watched_at": 1_700_000_100_000i64 },
            ],
        }));
        assert!(result["progress"]["tt1"].is_object());

        let result = merge(json!({
            "progress": {},
            "watched": { "tt1:1:2": true },
            "library": [],
            "addonMetas": {},
            "watchProgress": [],
            "watchHistory": [],
        }));
        assert_eq!(result["watched"]["tt1:1:2"], json!(true));
    }

    #[test]
    fn active_remote_progress_clears_conflicting_watched_flags() {
        let result = merge(json!({
            "progress": {},
            "watched": { "tt1:1:2": true, "tt9": true },
            "library": [],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "series", "video_id": "vid1",
                "position": 500_000, "duration": 1_000_000,
                "season": 1, "episode": 2, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": [],
        }));
        assert!(result["watched"].get("tt1:1:2").is_none());
        assert_eq!(result["watched"]["tt9"], json!(true));
    }

    #[test]
    fn resolved_up_next_saved_at_ignores_history_watched_at() {
        let result = merge(json!({
            "progress": {},
            "watched": {},
            "library": [],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "series", "video_id": "tt1:2:1",
                "position": 0, "duration": 1_000_000,
                "season": 2, "episode": 1, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": [
                { "content_id": "tt1", "content_type": "series", "season": 1, "episode": 9, "watched_at": 1_700_000_500_000i64 },
            ],
        }));
        let entry = &result["progress"]["tt1"];
        assert_eq!(entry["continueWatchingBadge"], json!("upNext"));
        assert_eq!(entry["savedAt"], json!(iso_from_ms(1_700_000_000_000)));
    }

    #[test]
    fn missing_history_keeps_local_watched_untouched() {
        let result = merge(json!({
            "progress": {},
            "watched": { "vid1": true },
            "library": [],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "movie", "video_id": "vid1",
                "position": 500_000, "duration": 1_000_000, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": null,
        }));
        assert_eq!(result["watched"]["vid1"], json!(true));
        assert!(result["progress"].get("tt1").is_none());
    }

    #[test]
    fn mid_progress_entry_is_not_marked_up_next() {
        let result = merge(json!({
            "progress": {},
            "watched": {},
            "library": [{ "content_id": "tt1", "name": "Show", "poster": "p.jpg" }],
            "addonMetas": {},
            "watchProgress": [{
                "content_id": "tt1", "content_type": "movie", "video_id": "vid1",
                "position": 600_000, "duration": 1_200_000, "last_watched": 1_700_000_000_000i64,
            }],
            "watchHistory": [],
        }));
        let entry = &result["progress"]["tt1"];
        assert!(entry.get("continueWatchingBadge").is_none());
        assert_eq!(entry["timeOffset"], json!(600));
        assert_eq!(entry["meta"]["name"], json!("Show"));
        assert_eq!(entry["meta"]["poster"], json!("p.jpg"));
    }
}
