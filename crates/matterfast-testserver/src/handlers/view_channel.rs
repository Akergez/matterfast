use axum::Json;
use serde_json::{json, Value};

pub(crate) async fn view_channel() -> Json<Value> {
    Json(json!({"status": "OK", "last_viewed_at_times": {}}))
}
