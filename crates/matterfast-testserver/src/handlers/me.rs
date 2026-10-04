use axum::Json;
use serde_json::Value;

use crate::constants::ME;
use crate::model::user;

pub(crate) async fn me() -> Json<Value> {
    Json(user(ME))
}
