use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::Value;

use crate::app::App;
use crate::constants::ME;

pub(crate) async fn create_post(
    State(app): State<Arc<App>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let channel_id = body["channel_id"].as_str().unwrap_or_default();
    let message = body["message"].as_str().unwrap_or_default();
    let root_id = body["root_id"].as_str().unwrap_or_default();
    let pending = body["pending_post_id"].as_str().unwrap_or_default();
    let post = app.add_post_with_pending(channel_id, ME, message, root_id, pending);
    app.post_channel_event(&post, vec![]);
    Json(post)
}
