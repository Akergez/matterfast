use axum::Json;
use serde_json::Value;

use crate::model::user;

pub(crate) async fn users_by_ids(Json(ids): Json<Vec<String>>) -> Json<Value> {
    Json(Value::Array(ids.iter().map(|i| user(i)).collect()))
}
