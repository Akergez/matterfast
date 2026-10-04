use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;

use crate::app::{post_list, App};

pub(crate) async fn post_thread(
    State(app): State<Arc<App>>,
    Path(post_id): Path<String>,
) -> Json<Value> {
    Json(post_list(&app, |p| {
        p["id"] == post_id.as_str() || p["root_id"] == post_id.as_str()
    }))
}
