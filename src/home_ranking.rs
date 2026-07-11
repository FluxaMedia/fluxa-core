use crate::content_identity::{imdb_id, normalized_billboard_title};
use crate::search_plan::resolve_transport_url_json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

const CORE_SHELF_KEYS: &[&str] = &[
    "action",
    "adventure",
    "aksiyon",
    "macera",
    "sci fi",
    "science fiction",
    "bilim kurgu",
    "fantasy",
    "fantastik",
    "thriller",
    "gerilim",
    "crime",
    "suc",
    "comedy",
    "komedi",
    "drama",
    "dram",
    "family",
    "aile",
    "kids",
    "cocuk",
    "anime",
    "mini series",
    "mini dizi",
];

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeHomeCategory {
    name: String,
    items: Vec<Value>,
    id: String,
    #[serde(rename = "type")]
    content_type: String,
    semantic_name: Option<String>,
    movie_genre: Option<String>,
    series_genre: Option<String>,
    skip: Option<i32>,
    can_load_more: Option<bool>,
    catalog_id: Option<String>,
    addon_transport_url: Option<String>,
    addon_genre: Option<String>,
    catalog_sources: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HomeOptimizeRequest {
    categories: Vec<NativeHomeCategory>,
    preferred_order_labels: Vec<String>,
    preferred_genres: HashMap<String, i32>,
    preferred_types: HashMap<String, i32>,
    priority_labels: HomePriorityLabels,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HomePriorityLabels {
    trending_now: String,
    popular_for_you: String,
    most_watched: String,
}

fn meta_text<'a>(meta: &'a Value, key: &str) -> &'a str {
    meta.get(key).and_then(Value::as_str).unwrap_or("")
}

fn meta_i64(meta: &Value, key: &str) -> Option<i64> {
    meta.get(key).and_then(Value::as_i64)
}

fn meta_string_array(meta: &Value, key: &str) -> Vec<String> {
    meta.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn category_semantic_name(category: &NativeHomeCategory) -> &str {
    category
        .semantic_name
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(&category.name)
}

pub(crate) fn normalize_home_key(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut last_space = false;
    for ch in value.to_lowercase().chars() {
        let normalized = match ch {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            ch if ch.is_ascii_alphanumeric() => ch,
            _ => ' ',
        };
        if normalized == ' ' {
            if !last_space {
                output.push(' ');
                last_space = true;
            }
        } else {
            output.push(normalized);
            last_space = false;
        }
    }
    output.trim().to_string()
}

fn semantic_score(category: &NativeHomeCategory, item: &Value) -> i32 {
    let category_keys = [
        Some(category.name.as_str()),
        Some(category_semantic_name(category)),
        category.addon_genre.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(normalize_home_key)
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>();
    let genre_score = meta_string_array(item, "genres")
        .into_iter()
        .map(|genre| normalize_home_key(&genre))
        .filter(|genre| {
            category_keys
                .iter()
                .any(|key| key == genre || key.contains(genre) || genre.contains(key))
        })
        .count() as i32
        * 4;
    let title_score = [meta_text(item, "name"), meta_text(item, "originalName")]
        .into_iter()
        .map(normalize_home_key)
        .filter(|title| {
            category_keys
                .iter()
                .any(|key| !key.is_empty() && title.contains(key))
        })
        .count() as i32
        * 2;
    genre_score + title_score
}

fn curated_items(category: &NativeHomeCategory) -> Vec<Value> {
    let mut values = category
        .items
        .iter()
        .map(|item| (item.clone(), semantic_score(category, item)))
        .collect::<Vec<_>>();
    values.sort_by(|(left, left_score), (right, right_score)| {
        right_score
            .cmp(left_score)
            .then_with(|| {
                meta_i64(left, "rank")
                    .unwrap_or(i64::MAX)
                    .cmp(&meta_i64(right, "rank").unwrap_or(i64::MAX))
            })
            .then_with(|| {
                meta_text(right, "imdbRating")
                    .parse::<f32>()
                    .unwrap_or(0.0)
                    .partial_cmp(&meta_text(left, "imdbRating").parse::<f32>().unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter_map(|(item, _)| {
            let id = meta_text(&item, "id").to_string();
            if seen.insert(id) {
                Some(item)
            } else {
                None
            }
        })
        .take(24)
        .collect()
}

pub(crate) fn curate_home_items_json(category_json: &str) -> Option<String> {
    let category = serde_json::from_str::<NativeHomeCategory>(category_json).ok()?;
    serde_json::to_string(&curated_items(&category)).ok()
}

fn is_pinned(category: &NativeHomeCategory) -> bool {
    category.id == "library"
        || category.id == "watchlist"
        || category.id == "continue_watching"
        || category.content_type == "collection"
        || category.content_type == "collection_folder"
}

fn priority_boost(category: &NativeHomeCategory, labels: &HomePriorityLabels) -> i32 {
    let key = normalize_home_key(category_semantic_name(category));
    if key.contains(&normalize_home_key(&labels.trending_now)) {
        40
    } else if key.contains(&normalize_home_key(&labels.popular_for_you)) {
        32
    } else if key.contains(&normalize_home_key(&labels.most_watched)) {
        28
    } else if key.contains("new") || key.contains("yeni") {
        16
    } else {
        0
    }
}

fn personalization_score(
    category: &NativeHomeCategory,
    preferred_genres: &HashMap<String, i32>,
    preferred_types: &HashMap<String, i32>,
    labels: &HomePriorityLabels,
) -> i32 {
    let type_affinity = category
        .items
        .iter()
        .map(|item| {
            preferred_types
                .get(meta_text(item, "type"))
                .copied()
                .unwrap_or(0)
        })
        .sum::<i32>()
        * 12;
    let genre_affinity = category
        .items
        .iter()
        .flat_map(|item| meta_string_array(item, "genres"))
        .map(|genre| {
            preferred_genres
                .get(&normalize_home_key(&genre))
                .copied()
                .unwrap_or(0)
        })
        .sum::<i32>()
        * 10;
    let unique_top_items = category
        .items
        .iter()
        .take(10)
        .map(|item| meta_text(item, "id").to_string())
        .collect::<HashSet<_>>()
        .len() as i32
        * 8;
    let reason_boost = category
        .items
        .iter()
        .filter(|item| !meta_text(item, "reason").is_empty())
        .count() as i32
        * 14;
    type_affinity
        + genre_affinity
        + unique_top_items
        + reason_boost
        + priority_boost(category, labels)
}

fn overlap_ratio(first: &NativeHomeCategory, second: &NativeHomeCategory) -> f32 {
    let first_ids = first
        .items
        .iter()
        .take(12)
        .map(|item| meta_text(item, "id").to_string())
        .collect::<HashSet<_>>();
    let second_ids = second
        .items
        .iter()
        .take(12)
        .map(|item| meta_text(item, "id").to_string())
        .collect::<HashSet<_>>();
    if first_ids.is_empty() || second_ids.is_empty() {
        return 0.0;
    }
    first_ids.intersection(&second_ids).count() as f32
        / first_ids.len().min(second_ids.len()) as f32
}

pub(crate) fn home_overlap_ratio_json(first_json: &str, second_json: &str) -> Option<f32> {
    let first = serde_json::from_str::<NativeHomeCategory>(first_json).ok()?;
    let second = serde_json::from_str::<NativeHomeCategory>(second_json).ok()?;
    Some(overlap_ratio(&first, &second))
}

fn is_core_genre_shelf(category: &NativeHomeCategory) -> bool {
    if category.movie_genre.is_some()
        || category.series_genre.is_some()
        || category.addon_genre.is_some()
    {
        return true;
    }
    let key = normalize_home_key(category_semantic_name(category));
    CORE_SHELF_KEYS
        .iter()
        .any(|candidate| key == *candidate || key.contains(candidate))
}

fn cluster_key(category: &NativeHomeCategory) -> Option<String> {
    if let Some(genre) = category.movie_genre.as_deref() {
        return Some(format!("movie:{}", normalize_home_key(genre)));
    }
    if let Some(genre) = category.series_genre.as_deref() {
        return Some(format!("series:{}", normalize_home_key(genre)));
    }
    if let Some(genre) = category.addon_genre.as_deref() {
        return Some(format!("addon:{}", normalize_home_key(genre)));
    }
    let key = normalize_home_key(category_semantic_name(category));
    CORE_SHELF_KEYS
        .iter()
        .find(|candidate| key == **candidate || key.contains(*candidate))
        .map(|value| (*value).to_string())
}

fn cluster_overlap_ratio(first: &NativeHomeCategory, second: &NativeHomeCategory) -> f32 {
    let Some(first_cluster) = cluster_key(first) else {
        return 0.0;
    };
    let Some(second_cluster) = cluster_key(second) else {
        return 0.0;
    };
    if first_cluster == second_cluster {
        overlap_ratio(first, second)
    } else {
        0.0
    }
}

pub(crate) fn home_personalization_score_json(
    category_json: &str,
    preferred_genres_json: &str,
    preferred_types_json: &str,
    priority_labels_json: &str,
) -> Option<i32> {
    let category = serde_json::from_str::<NativeHomeCategory>(category_json).ok()?;
    let preferred_genres =
        serde_json::from_str::<HashMap<String, i32>>(preferred_genres_json).ok()?;
    let preferred_types =
        serde_json::from_str::<HashMap<String, i32>>(preferred_types_json).ok()?;
    let labels = serde_json::from_str::<HomePriorityLabels>(priority_labels_json).ok()?;
    Some(personalization_score(
        &category,
        &preferred_genres,
        &preferred_types,
        &labels,
    ))
}

pub(crate) fn home_prioritize_rows_json(
    categories_json: &str,
    preferred_order_labels_json: &str,
    preferred_genres_json: &str,
    preferred_types_json: &str,
    priority_labels_json: &str,
) -> Option<String> {
    let mut categories = serde_json::from_str::<Vec<NativeHomeCategory>>(categories_json).ok()?;
    let preferred_order_labels =
        serde_json::from_str::<Vec<String>>(preferred_order_labels_json).ok()?;
    let preferred_genres =
        serde_json::from_str::<HashMap<String, i32>>(preferred_genres_json).ok()?;
    let preferred_types =
        serde_json::from_str::<HashMap<String, i32>>(preferred_types_json).ok()?;
    let labels = serde_json::from_str::<HomePriorityLabels>(priority_labels_json).ok()?;
    let preferred_order = preferred_order_labels
        .iter()
        .map(|value| normalize_home_key(value))
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| {
        let left_index = preferred_order
            .iter()
            .position(|key| key == &normalize_home_key(category_semantic_name(left)))
            .unwrap_or(usize::MAX);
        let right_index = preferred_order
            .iter()
            .position(|key| key == &normalize_home_key(category_semantic_name(right)))
            .unwrap_or(usize::MAX);
        left_index.cmp(&right_index).then_with(|| {
            personalization_score(right, &preferred_genres, &preferred_types, &labels).cmp(
                &personalization_score(left, &preferred_genres, &preferred_types, &labels),
            )
        })
    });
    serde_json::to_string(&categories).ok()
}

pub(crate) fn optimize_home_rows_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<HomeOptimizeRequest>(request_json).ok()?;
    if request.categories.is_empty() {
        return Some("[]".to_string());
    }
    let pinned = distinct_categories(
        request
            .categories
            .iter()
            .filter(|category| is_pinned(category))
            .cloned(),
    );
    let candidates = sorted_unpinned_candidates(&request);
    let kept = select_diverse_categories(&candidates);
    let fallback = fallback_categories(candidates, &kept);

    let mut output = pinned;
    output.extend(kept);
    output.extend(fallback);
    let limit = 24 + output_pinned_count(&output);
    let output = distinct_categories(output)
        .into_iter()
        .take(limit)
        .collect::<Vec<_>>();
    serde_json::to_string(&output).ok()
}

// Unpinned categories, curated down to their top items and sorted by the
// caller's preferred order first, personalization score second.
fn sorted_unpinned_candidates(request: &HomeOptimizeRequest) -> Vec<NativeHomeCategory> {
    let mut candidates = distinct_categories(
        request
            .categories
            .iter()
            .filter(|category| !is_pinned(category))
            .cloned(),
    )
    .into_iter()
    .map(|mut category| {
        category.items = curated_items(&category);
        category
    })
    .filter(|category| category.items.len() >= 4)
    .collect::<Vec<_>>();
    let preferred_order = request
        .preferred_order_labels
        .iter()
        .map(|value| normalize_home_key(value))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_index = preferred_order
            .iter()
            .position(|key| key == &normalize_home_key(category_semantic_name(left)))
            .unwrap_or(usize::MAX);
        let right_index = preferred_order
            .iter()
            .position(|key| key == &normalize_home_key(category_semantic_name(right)))
            .unwrap_or(usize::MAX);
        left_index.cmp(&right_index).then_with(|| {
            personalization_score(
                right,
                &request.preferred_genres,
                &request.preferred_types,
                &request.priority_labels,
            )
            .cmp(&personalization_score(
                left,
                &request.preferred_genres,
                &request.preferred_types,
                &request.priority_labels,
            ))
        })
    });
    candidates
}

// Greedily keep candidates that are either a core genre shelf or don't overlap
// too much with what's already kept, so the final list isn't redundant.
fn select_diverse_categories(candidates: &[NativeHomeCategory]) -> Vec<NativeHomeCategory> {
    let mut kept = Vec::<NativeHomeCategory>::new();
    for category in candidates.iter() {
        let overlap = kept
            .iter()
            .map(|existing| overlap_ratio(existing, category))
            .fold(0.0, f32::max);
        let cluster_overlap = kept
            .iter()
            .map(|existing| cluster_overlap_ratio(existing, category))
            .fold(0.0, f32::max);
        let min_unique = category
            .items
            .iter()
            .take(12)
            .map(|item| meta_text(item, "id").to_string())
            .collect::<HashSet<_>>()
            .len();
        if min_unique < 5 {
            continue;
        }
        if is_core_genre_shelf(category)
            || (overlap < 0.68 && cluster_overlap < 0.52)
            || kept.len() < 8
        {
            kept.push(category.clone());
        }
    }
    kept
}

// Fill remaining slots (up to 24 total) from leftover candidates that still
// don't overlap too much with anything already kept.
fn fallback_categories(
    candidates: Vec<NativeHomeCategory>,
    kept: &[NativeHomeCategory],
) -> Vec<NativeHomeCategory> {
    candidates
        .into_iter()
        .filter(|candidate| {
            kept.iter().all(|existing| existing.id != candidate.id)
                && kept.iter().all(|existing| {
                    overlap_ratio(existing, candidate) < 0.68
                        && cluster_overlap_ratio(existing, candidate) < 0.52
                })
        })
        .take(24usize.saturating_sub(kept.len()))
        .collect::<Vec<_>>()
}

fn output_pinned_count(categories: &[NativeHomeCategory]) -> usize {
    categories
        .iter()
        .filter(|category| is_pinned(category))
        .count()
}

fn distinct_categories<I>(categories: I) -> Vec<NativeHomeCategory>
where
    I: IntoIterator<Item = NativeHomeCategory>,
{
    let mut seen = HashSet::new();
    categories
        .into_iter()
        .filter(|category| seen.insert(category.id.clone()))
        .collect()
}

fn has_backdrop_candidate(meta: &Value) -> bool {
    let background = meta_text(meta, "background");
    !background.is_empty() && !background.eq_ignore_ascii_case(meta_text(meta, "poster"))
}

fn score_candidate(meta: &Value, days_since_release: Option<i64>) -> i32 {
    let release_boost = match days_since_release {
        None => 0,
        Some(days) if days < 0 => 40,
        Some(days) if days <= 14 => 440,
        Some(days) if days <= 45 => 280,
        Some(days) if days <= 120 => 120,
        Some(_) => 0,
    };
    let type_boost = if meta_text(meta, "type") == "series" {
        320
    } else {
        140
    };
    let rank_boost = meta_i64(meta, "rank")
        .map(|rank| (220 - ((rank as i32 - 1) * 18)).max(0))
        .unwrap_or(0);
    let rating_boost = (meta_text(meta, "imdbRating").parse::<f32>().unwrap_or(0.0) * 22.0) as i32;
    let recommendation_boost = if meta_text(meta, "reason").is_empty() {
        0
    } else {
        180
    };
    let editorial_boost = if meta_text(meta, "reason") == "EDITORIAL_SPOTLIGHT" {
        520
    } else {
        0
    };
    let backdrop_boost = if has_backdrop_candidate(meta) {
        260
    } else if !meta_text(meta, "poster").is_empty() {
        40
    } else {
        -240
    };
    type_boost
        + release_boost
        + rank_boost
        + rating_boost
        + recommendation_boost
        + editorial_boost
        + backdrop_boost
}

fn billboard_key_value(meta: &Value) -> String {
    let id = meta_text(meta, "id");
    if let Some(iid) = imdb_id(id) {
        return format!("{}:{iid}", meta_text(meta, "type"));
    }
    let name = meta
        .get("originalName")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| meta_text(meta, "name"));
    let year = meta_text(meta, "releaseInfo")
        .get(0..4)
        .or_else(|| meta_text(meta, "released").get(0..4))
        .unwrap_or("");
    format!(
        "{}:{}:{year}",
        meta_text(meta, "type"),
        normalized_billboard_title(name)
    )
}

fn title_key_value(meta: &Value) -> String {
    let name = meta
        .get("originalName")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| meta_text(meta, "name"));
    normalized_billboard_title(name)
}

fn distinct_by_billboard_key(items: Vec<Value>) -> Vec<Value> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|m| seen.insert(billboard_key_value(m)))
        .collect()
}

fn distinct_by_title_key(items: Vec<Value>) -> Vec<Value> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|m| seen.insert(title_key_value(m)))
        .collect()
}

fn billboard_visual_score(meta: &Value) -> i32 {
    let mut score = 0i32;
    if has_backdrop_candidate(meta) {
        score += 320;
    } else {
        score -= 160;
    }
    if !meta_text(meta, "logo").is_empty() {
        score += 120;
    }
    if !meta_text(meta, "description").is_empty() {
        score += 30;
    }
    score
}

pub(crate) fn build_billboard_pool_json(
    enriched_json: &str,
    candidates_json: &str,
) -> Option<String> {
    let enriched: Vec<Value> = serde_json::from_str(enriched_json).ok()?;
    let candidates: Vec<Value> = serde_json::from_str(candidates_json).ok()?;

    let enriched_by_key: HashMap<String, Value> = enriched
        .iter()
        .map(|m| (billboard_key_value(m), m.clone()))
        .collect();

    // Editorial picks: prefer the enriched version, fall back to original when it has artwork.
    let editorial_raw: Vec<Value> = candidates
        .iter()
        .filter(|m| meta_text(m, "reason") == "EDITORIAL_SPOTLIGHT")
        .filter_map(|m| {
            let key = billboard_key_value(m);
            enriched_by_key.get(&key).cloned().or_else(|| {
                if has_backdrop_candidate(m) || !meta_text(m, "poster").is_empty() {
                    Some(m.clone())
                } else {
                    None
                }
            })
        })
        .collect();

    let mut editorial = editorial_raw;
    editorial.sort_by_key(|item| std::cmp::Reverse(score_candidate(item, None)));
    let editorial: Vec<Value> = distinct_by_title_key(editorial)
        .into_iter()
        .take(3)
        .collect();

    // Ranked pool: merge enriched + candidates, deduplicate, filter, sort by score+visual.
    let combined: Vec<Value> = enriched.into_iter().chain(candidates).collect();
    let combined = distinct_by_title_key(distinct_by_billboard_key(combined));
    let mut ranked: Vec<Value> = combined
        .into_iter()
        .filter(|m| has_backdrop_candidate(m) || !meta_text(m, "poster").is_empty())
        .collect();
    ranked.sort_by(|a, b| {
        let sb = score_candidate(b, None) + billboard_visual_score(b);
        let sa = score_candidate(a, None) + billboard_visual_score(a);
        sb.cmp(&sa)
    });

    let series: Vec<Value> = ranked
        .iter()
        .filter(|m| meta_text(m, "type") == "series")
        .take(8)
        .cloned()
        .collect();
    let movies: Vec<Value> = ranked
        .iter()
        .filter(|m| meta_text(m, "type") == "movie")
        .take(3)
        .cloned()
        .collect();

    let preferred: Vec<Value> = distinct_by_title_key(distinct_by_billboard_key(
        editorial.into_iter().chain(series).chain(movies).collect(),
    ));

    let final_pool: Vec<Value> = if preferred.len() >= 10 {
        preferred.into_iter().take(10).collect()
    } else {
        let preferred_keys: HashSet<String> = preferred.iter().map(billboard_key_value).collect();
        let preferred_titles: HashSet<String> = preferred.iter().map(title_key_value).collect();
        let extras = ranked.into_iter().filter(|m| {
            !preferred_keys.contains(&billboard_key_value(m))
                && !preferred_titles.contains(&title_key_value(m))
        });
        preferred.into_iter().chain(extras).take(10).collect()
    };

    serde_json::to_string(&final_pool).ok()
}

fn iso_date_part(date_str: &str) -> Option<&str> {
    let s = date_str.trim();
    let date_part = s.get(..10)?;
    let b = date_part.as_bytes();
    if b[4] == b'-' && b[7] == b'-' {
        Some(date_part)
    } else {
        None
    }
}

fn is_upcoming_date(date_str: &str, today_iso: &str) -> bool {
    iso_date_part(date_str).is_some_and(|d| d > today_iso)
}

const RANKED_CATALOG_IDS: &[&str] = &["trending", "popular", "top", "now_playing"];

pub(crate) fn normalize_home_catalog_items_json(
    items_json: &str,
    catalog_id: &str,
    genre: Option<&str>,
    today_iso: &str,
) -> Option<String> {
    let items: Vec<Value> = serde_json::from_str(items_json).ok()?;
    let assign_rank =
        genre.map(|g| g.is_empty()).unwrap_or(true) && RANKED_CATALOG_IDS.contains(&catalog_id);

    let mut rank: i64 = 0;
    let result: Vec<Value> = items
        .into_iter()
        .filter_map(|mut item| {
            let released = item.get("released").and_then(Value::as_str).unwrap_or("");
            if is_upcoming_date(released, today_iso) {
                return None;
            }
            if assign_rank {
                rank += 1;
                if let Some(obj) = item.as_object_mut() {
                    obj.insert("rank".to_string(), json!(rank));
                }
            }
            Some(item)
        })
        .collect();

    serde_json::to_string(&result).ok()
}

pub(crate) fn build_home_collection_shelves_json(
    profile_json: &str,
    addons_json: &str,
) -> Option<String> {
    let profile: Value = serde_json::from_str(profile_json).ok()?;
    let collections =
        match profile.get("libraryCollections").and_then(Value::as_array) {
            Some(c) => c,
            None => return serde_json::to_string(
                &json!({ "pinnedShelves": [], "regularShelves": [], "hiddenFolderCategories": [] }),
            )
            .ok(),
        };

    let mut pinned: Vec<Value> = Vec::new();
    let mut regular: Vec<Value> = Vec::new();
    let mut hidden: Vec<Value> = Vec::new();

    for (ci, col) in collections.iter().enumerate() {
        let c = match col.as_object() {
            Some(o) => o,
            None => continue,
        };
        if !c
            .get("showOnHome")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let folders = c
            .get("folders")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if folders.is_empty() {
            continue;
        }

        let mut tiles: Vec<Value> = Vec::new();

        for (fi, f) in folders.iter().enumerate() {
            let folder = match f.as_object() {
                Some(o) => o,
                None => continue,
            };
            let folder_title = folder
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if folder_title.is_empty() {
                continue;
            }
            let folder_id = folder
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("col{ci}_f{fi}"));

            let resolved = resolve_folder_catalog_sources(folder, addons_json);
            if !resolved.is_empty() {
                hidden.push(hidden_folder_category(
                    &folder_id,
                    &folder_title,
                    folder,
                    resolved,
                ));
            }
            tiles.push(folder_tile(&folder_id, &folder_title, folder));
        }

        if tiles.is_empty() {
            continue;
        }

        let shelf_id = c
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("col{ci}"));
        let shelf = json!({
            "id": shelf_id,
            "name": c.get("title").and_then(Value::as_str).unwrap_or(""),
            "type": "collection",
            "items": tiles,
            "canLoadMore": false,
        });

        if c.get("pinToTop").and_then(Value::as_bool).unwrap_or(false) {
            pinned.push(shelf);
        } else {
            regular.push(shelf);
        }
    }

    serde_json::to_string(&json!({
        "pinnedShelves": pinned,
        "regularShelves": regular,
        "hiddenFolderCategories": hidden,
    }))
    .ok()
}

// A folder's catalog sources, preferring its explicit catalogSources list and
// falling back to a single source built from its own catalogId.
fn resolve_folder_catalog_sources(folder: &Map<String, Value>, addons_json: &str) -> Vec<Value> {
    let mut resolved: Vec<Value> = Vec::new();
    if let Some(sources) = folder.get("catalogSources").and_then(Value::as_array) {
        for s in sources {
            if s.get("catalogId").and_then(Value::as_str).is_none() {
                continue;
            }
            if let Some(t_url) = resolve_transport_url_json(&s.to_string(), addons_json)
                .and_then(|json| serde_json::from_str::<String>(&json).ok())
            {
                let catalog_id = s.get("catalogId").and_then(Value::as_str).unwrap_or("");
                let content_type = s.get("type").and_then(Value::as_str).unwrap_or("movie");
                let mut entry =
                    json!({ "transportUrl": t_url, "catalogId": catalog_id, "type": content_type });
                if let Some(g) = folder.get("genre").and_then(Value::as_str) {
                    entry["genre"] = Value::String(g.to_string());
                }
                resolved.push(entry);
            }
        }
    }

    if resolved.is_empty() {
        if let Some(sources) = folder.get("sources").and_then(Value::as_array) {
            for source in sources {
                let provider = source.get("provider").and_then(Value::as_str).unwrap_or("");
                if (provider == "trakt"
                    && source.get("traktListId").and_then(Value::as_i64).is_some())
                    || (provider == "tmdb"
                        && source
                            .get("tmdbSourceType")
                            .and_then(Value::as_str)
                            .is_some())
                {
                    resolved.push(source.clone());
                }
            }
        }
    }

    if resolved.is_empty() {
        if let Some(catalog_id) = folder.get("catalogId").and_then(Value::as_str) {
            let src = json!({ "catalogId": catalog_id, "type": "movie" });
            if let Some(t_url) = resolve_transport_url_json(&src.to_string(), addons_json)
                .and_then(|json| serde_json::from_str::<String>(&json).ok())
            {
                let mut entry =
                    json!({ "transportUrl": t_url, "catalogId": catalog_id, "type": "movie" });
                if let Some(g) = folder.get("genre").and_then(Value::as_str) {
                    entry["genre"] = Value::String(g.to_string());
                }
                resolved.push(entry);
            }
        }
    }
    resolved
}

fn hidden_folder_category(
    folder_id: &str,
    folder_title: &str,
    folder: &Map<String, Value>,
    resolved: Vec<Value>,
) -> Value {
    let mut hcat = json!({
        "id": folder_id,
        "name": folder_title,
        "type": "collection_folder",
        "items": [],
        "catalogSources": resolved,
        "canLoadMore": false,
    });
    if let Some(g) = folder.get("genre").and_then(Value::as_str) {
        hcat["addonGenre"] = Value::String(g.to_string());
    }
    hcat
}

fn folder_tile(folder_id: &str, folder_title: &str, folder: &Map<String, Value>) -> Value {
    let img_url = folder
        .get("coverImageUrl")
        .and_then(Value::as_str)
        .or_else(|| folder.get("imageUrl").and_then(Value::as_str))
        .unwrap_or("");
    let bg_url = folder
        .get("heroBackdropUrl")
        .and_then(Value::as_str)
        .unwrap_or(img_url);
    let mut tile = json!({
        "id": folder_id,
        "type": "catalog_folder",
        "name": folder_title,
        "poster": if img_url.is_empty() { Value::Null } else { Value::String(img_url.to_string()) },
        "background": if bg_url.is_empty() { Value::Null } else { Value::String(bg_url.to_string()) },
        "reason": folder.get("shape").and_then(Value::as_str).unwrap_or("poster"),
    });
    if let Some(logo) = folder.get("titleLogoUrl").and_then(Value::as_str) {
        tile["logo"] = Value::String(logo.to_string());
    }
    if let Some(info) = folder.get("catalogTitle").and_then(Value::as_str) {
        tile["releaseInfo"] = Value::String(info.to_string());
    }
    if let Some(gif) = folder.get("focusGifUrl").and_then(Value::as_str) {
        tile["focusGifUrl"] = Value::String(gif.to_string());
    }
    tile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_collection_shelves_filter_hidden_collections_and_resolve_catalog_sources() {
        let profile = json!({
            "libraryCollections": [
                {
                    "id": "col1",
                    "title": "My Collection",
                    "showOnHome": true,
                    "pinToTop": true,
                    "folders": [
                        {
                            "id": "f1",
                            "title": "Action",
                            "coverImageUrl": "https://img.example/cover.jpg",
                            "focusGifUrl": "https://img.example/focus.gif",
                            "focusGifEnabled": false,
                            "catalogSources": [{ "catalogId": "top", "type": "movie" }],
                        }
                    ],
                },
                {
                    "id": "col2",
                    "title": "Not Shown",
                    "showOnHome": false,
                    "folders": [{ "id": "f2", "title": "Hidden", "catalogId": "top" }],
                },
            ],
        });
        let addons = json!([
            {
                "transportUrl": "https://addon.example/manifest.json",
                "manifest": { "id": "addon.example", "catalogs": [{ "id": "top", "type": "movie" }] },
            }
        ]);

        let result = build_home_collection_shelves_json(&profile.to_string(), &addons.to_string())
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .expect("shelves");

        assert!(result["regularShelves"].as_array().unwrap().is_empty());
        let pinned = result["pinnedShelves"].as_array().unwrap();
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0]["id"], "col1");
        assert_eq!(pinned[0]["items"][0]["id"], "f1");
        assert_eq!(
            pinned[0]["items"][0]["poster"],
            "https://img.example/cover.jpg"
        );
        assert_eq!(
            pinned[0]["items"][0]["focusGifUrl"],
            "https://img.example/focus.gif"
        );

        let hidden = result["hiddenFolderCategories"].as_array().unwrap();
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0]["id"], "f1");
        assert_eq!(
            hidden[0]["catalogSources"][0]["transportUrl"],
            "https://addon.example/manifest.json"
        );
    }
}
