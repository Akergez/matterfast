use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::Value;

use crate::app::{post_list, App};

pub(crate) async fn unread_posts(
    State(app): State<Arc<App>>,
    Path(channel_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let crt = q.get("collapsedThreads").map(String::as_str) == Some("true");
    Json(post_list(&app, |p| {
        p["channel_id"] == channel_id.as_str()
            && (!crt || p["root_id"].as_str().unwrap_or_default().is_empty())
    }))
}
