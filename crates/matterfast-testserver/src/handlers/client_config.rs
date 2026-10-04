use axum::Json;
use serde_json::{json, Value};

pub(crate) async fn client_config() -> Json<Value> {
    Json(json!({
        "Version": "11.11.0",
        "SiteName": "Test Mattermost",
        "TeammateNameDisplay": "full_name",
        // Collapsed reply threads on, so the client exercises the CRT path.
        "CollapsedThreads": "default_on",
        "EnableCustomEmoji": "true",
        "PostPriority": "true",
    }))
}
