use serde_json::{json, Value};

pub(crate) fn oauth_request_plan_json(request_json: &str) -> Option<String> {
    let request: Value = serde_json::from_str(request_json).ok()?;
    let service = request.get("service")?.as_str()?;
    let operation = request.get("operation")?.as_str()?;
    let client_id = request
        .get("clientId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let client_secret = request
        .get("clientSecret")
        .and_then(Value::as_str)
        .unwrap_or("");
    let code = request.get("code").and_then(Value::as_str).unwrap_or("");
    let refresh_token = request
        .get("refreshToken")
        .and_then(Value::as_str)
        .unwrap_or("");
    let (url, body) = match (service, operation) {
        ("trakt", "device_start") => (
            "https://api.trakt.tv/oauth/device/code",
            json!({"client_id": client_id}),
        ),
        ("trakt", "device_poll") => (
            "https://api.trakt.tv/oauth/device/token",
            json!({"code": code, "client_id": client_id}),
        ),
        ("trakt", "exchange") => (
            "https://api.trakt.tv/oauth/token",
            json!({"code": code, "client_id": client_id, "client_secret": client_secret, "redirect_uri": "fluxa://oauth/trakt", "grant_type": "authorization_code"}),
        ),
        ("anilist", "exchange") => (
            "https://anilist.co/api/v2/oauth/token",
            json!({"grant_type": "authorization_code", "client_id": client_id, "client_secret": client_secret, "redirect_uri": "fluxa://oauth/anilist", "code": code}),
        ),
        ("anilist", "refresh") => (
            "https://anilist.co/api/v2/oauth/token",
            json!({"grant_type": "refresh_token", "client_id": client_id, "client_secret": client_secret, "refresh_token": refresh_token}),
        ),
        ("simkl", "exchange") => (
            "https://api.simkl.com/oauth/token",
            json!({"code": code, "client_id": client_id, "client_secret": client_secret, "redirect_uri": "fluxa://oauth/simkl", "grant_type": "authorization_code"}),
        ),
        _ => return None,
    };
    serde_json::to_string(&json!({"url": url, "body": body})).ok()
}

pub(crate) fn oauth_response_outcome(service: &str, operation: &str, status: u16) -> &'static str {
    if (200..300).contains(&status) {
        return "success";
    }
    if service == "trakt" && operation == "device_poll" {
        return if status == 400 || status == 429 {
            "pending"
        } else {
            "expired"
        };
    }
    "error"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_provider_requests_and_poll_outcomes() {
        let plan: Value = serde_json::from_str(&oauth_request_plan_json(r#"{"service":"trakt","operation":"exchange","clientId":"id","clientSecret":"secret","code":"code"}"#).unwrap()).unwrap();
        assert_eq!(plan["url"], "https://api.trakt.tv/oauth/token");
        assert_eq!(plan["body"]["redirect_uri"], "fluxa://oauth/trakt");
        assert_eq!(
            oauth_response_outcome("trakt", "device_poll", 429),
            "pending"
        );
        assert_eq!(
            oauth_response_outcome("trakt", "device_poll", 410),
            "expired"
        );
    }
}
