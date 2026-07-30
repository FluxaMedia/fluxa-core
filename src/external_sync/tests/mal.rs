use super::super::*;
use serde_json::{Value, json};

#[test]
fn mal_sync_policy_maps_auth_and_episode_updates() {
    assert_eq!(
        external_sync_response_action("mal", 401),
        "refresh_credentials"
    );
    assert_eq!(
        external_sync_response_action("simkl", 401),
        "clear_credentials"
    );
    assert_eq!(
        external_sync_refresh_retry_action(Some(401)),
        "clear_credentials"
    );
    let watched = mal_list_update_json(
        &json!({
            "meta": { "id": "mal:42", "type": "series", "episodesCount": 12 },
            "episodes": [{ "number": 12 }],
        })
        .to_string(),
        true,
    )
    .and_then(|value| serde_json::from_str::<Value>(&value).ok())
    .unwrap();
    assert_eq!(watched["status"], "completed");
}
