use axum::Json;
use serde_json::{json, Value};

pub(crate) async fn empty_object() -> Json<Value> {
    Json(json!({}))
}

pub(crate) async fn empty_array() -> Json<Value> {
    Json(json!([]))
}
