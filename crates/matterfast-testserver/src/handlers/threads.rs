use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::app::App;
use crate::constants::{ME, SARA};
use crate::model::user;

/// `GET /users/{me}/teams/{team}/threads` — the CRT inbox.
pub(crate) async fn threads(State(app): State<Arc<App>>) -> Json<Value> {
    let db = app.db.lock().unwrap();
    let roots: Vec<Value> = db
        .posts
        .iter()
        .map(|p| &p.v)
        .filter(|p| p["reply_count"].as_i64().unwrap_or(0) > 0)
        .map(|p| {
            json!({
                "id": p["id"],
                "reply_count": p["reply_count"],
                "last_reply_at": p["last_reply_at"],
                "last_viewed_at": 0,
                "unread_replies": p["reply_count"],
                "unread_mentions": 0,
                "is_following": true,
                "post": p,
                "participants": [user(SARA), user(ME)],
            })
        })
        .collect();
    Json(json!({
        "threads": roots,
        "total": 0,
        "total_unread_threads": 0,
        "total_unread_mentions": 0,
    }))
}
