use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryPlanRequest {
    repository_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryPlanRequest {
    repository_url: String,
    repository: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogRequest {
    repository_url: String,
    #[serde(default)]
    reference: Option<String>,
    tree: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackRequest {
    manifest_url: String,
    pack: Value,
}

struct GitHubRepository {
    owner: String,
    name: String,
}

/// Normalizes a GitHub repository pasted by a user and returns the first
/// platform-owned HTTP request needed to discover its default branch.
pub(crate) fn profile_avatar_pack_repository_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<RepositoryPlanRequest>(request_json).ok()?;
    let repository = parse_repository_url(&request.repository_url)?;
    serde_json::to_string(&json!({
        "owner": repository.owner,
        "repository": repository.name,
        "repositoryApiUrl": format!("https://api.github.com/repos/{}/{}", repository.owner, repository.name),
    }))
    .ok()
}

/// Creates the recursive GitHub tree request after the platform has fetched
/// the repository metadata returned by `profileAvatarPackRepositoryPlan`.
pub(crate) fn profile_avatar_pack_discovery_plan_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<DiscoveryPlanRequest>(request_json).ok()?;
    let repository = parse_repository_url(&request.repository_url)?;
    let reference = request
        .repository
        .get("default_branch")
        .or_else(|| request.repository.get("defaultBranch"))
        .and_then(Value::as_str)
        .filter(|value| valid_ref(value))?
        .to_string();
    serde_json::to_string(&json!({
        "reference": reference,
        "treeApiUrl": format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            repository.owner,
            repository.name,
            percent_encode(&reference),
        ),
    }))
    .ok()
}

/// Extracts every supported avatar-pack manifest from a GitHub recursive tree.
/// A pack is a `pack.json` or `json.pack` blob in any directory, allowing
/// repositories to group packs under arbitrary category paths.
pub(crate) fn profile_avatar_pack_catalog_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<CatalogRequest>(request_json).ok()?;
    let repository = parse_repository_url(&request.repository_url)?;
    let reference = request.reference.filter(|value| valid_ref(value))?;
    if request.tree.get("truncated").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let entries = request.tree.get("tree")?.as_array()?;
    let mut categories = Vec::new();
    let mut seen_paths = HashSet::new();

    for entry in entries {
        if entry.get("type").and_then(Value::as_str) != Some("blob") {
            continue;
        }
        let Some(path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        let Some((directory, filename)) = path.rsplit_once('/') else {
            continue;
        };
        if !matches!(
            filename.to_ascii_lowercase().as_str(),
            "pack.json" | "json.pack"
        ) || directory.is_empty()
            || !valid_path(path)
            || !seen_paths.insert(path.to_string())
        {
            continue;
        }
        let name = directory.rsplit('/').next().unwrap_or(directory);
        categories.push(json!({
            "name": name,
            "path": directory,
            "manifestUrl": raw_file_url(&repository, &reference, path),
        }));
    }

    categories.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .cmp(&right["path"].as_str().unwrap_or("").to_lowercase())
    });
    serde_json::to_string(&json!({
        "owner": repository.owner,
        "repository": repository.name,
        "reference": reference,
        "categories": categories,
    }))
    .ok()
}

/// Validates a fetched pack document before image URLs reach a platform image
/// loader. The source repository is intentionally not restricted here: packs
/// may legitimately host images on a CDN, but only HTTPS image URLs are safe.
pub(crate) fn profile_avatar_pack_json(request_json: &str) -> Option<String> {
    let request = serde_json::from_str::<PackRequest>(request_json).ok()?;
    if !is_https_url(&request.manifest_url) {
        return None;
    }
    let title = request
        .pack
        .get("title")
        .or_else(|| request.pack.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let images = request.pack.get("images")?.as_array()?;
    let mut avatars = Vec::new();
    let mut seen_urls = HashSet::new();
    for image in images {
        let Some(url) = image.get("url").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if !is_https_url(url) || !seen_urls.insert(url.to_string()) {
            continue;
        }
        let name = image
            .get("name")
            .or_else(|| image.get("title"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Avatar");
        avatars.push(json!({"name": name, "url": url}));
    }
    serde_json::to_string(&json!({
        "title": title,
        "manifestUrl": request.manifest_url,
        "avatars": avatars,
    }))
    .ok()
}

fn parse_repository_url(input: &str) -> Option<GitHubRepository> {
    let input = input.trim().trim_end_matches('/');
    let path = input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))
        .or_else(|| input.strip_prefix("github.com/"))
        .unwrap_or(input);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if parts.next().is_some() || !valid_repository_part(owner) || !valid_repository_part(name) {
        return None;
    }
    Some(GitHubRepository {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

fn valid_repository_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_ref(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 255
        && !value.contains([' ', '\\', '?', '#'])
        && !value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn raw_file_url(repository: &GitHubRepository, reference: &str, path: &str) -> String {
    let path = path
        .split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "https://raw.githubusercontent.com/{}/{}/{}/{}",
        repository.owner,
        repository.name,
        percent_encode(reference),
        path,
    )
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                vec![byte as char]
            } else {
                format!("%{byte:02X}").chars().collect()
            }
        })
        .collect()
}

fn is_https_url(value: &str) -> bool {
    let value = value.trim();
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    !rest.is_empty() && !rest.starts_with('/') && !rest.contains([' ', '\\', '\n', '\r'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_plan_accepts_github_urls_only() {
        let output: Value = serde_json::from_str(
            &profile_avatar_pack_repository_plan_json(
                r#"{"repositoryUrl":"https://github.com/eueueue292/Fusion-Profile-Avatars.git"}"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(output["owner"], "eueueue292");
        assert_eq!(output["repository"], "Fusion-Profile-Avatars");
        assert!(profile_avatar_pack_repository_plan_json(
            r#"{"repositoryUrl":"https://github.com.evil/a/b"}"#
        )
        .is_none());
    }

    #[test]
    fn catalog_discovers_nested_packs_and_builds_raw_urls() {
        let output: Value = serde_json::from_str(
            &profile_avatar_pack_catalog_json(
                r#"{
                    "repositoryUrl":"eueueue292/Fusion-Profile-Avatars",
                    "reference":"main",
                    "tree":{"tree":[
                        {"path":"Attack On Titan/pack.json","type":"blob"},
                        {"path":"Disney+/Marvel/json.pack","type":"blob"},
                        {"path":"README.md","type":"blob"}
                    ]}
                }"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(output["categories"].as_array().unwrap().len(), 2);
        assert_eq!(output["categories"][0]["path"], "Attack On Titan");
        assert_eq!(output["categories"][1]["name"], "Marvel");
        assert_eq!(
            output["categories"][1]["manifestUrl"],
            "https://raw.githubusercontent.com/eueueue292/Fusion-Profile-Avatars/main/Disney%2B/Marvel/json.pack"
        );
    }

    #[test]
    fn pack_parser_keeps_only_unique_https_avatars() {
        let output: Value = serde_json::from_str(
            &profile_avatar_pack_json(
                r#"{
                    "manifestUrl":"https://example.com/pack.json",
                    "pack":{"title":"Test pack","images":[
                        {"name":"A","url":"https://images.example/a.png"},
                        {"name":"Duplicate","url":"https://images.example/a.png"},
                        {"name":"Unsafe","url":"file:///tmp/avatar.png"}
                    ]}
                }"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(output["avatars"].as_array().unwrap().len(), 1);
        assert_eq!(output["avatars"][0]["name"], "A");
    }
}
