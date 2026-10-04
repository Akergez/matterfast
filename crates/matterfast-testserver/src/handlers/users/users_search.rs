use axum::Json;
use serde_json::Value;

use super::directory::directory;

/// `POST /users/search` — the whole directory, which is more than any client
/// has loaded.
pub(crate) async fn users_search(Json(body): Json<Value>) -> Json<Value> {
    Json(Value::Array(directory(body["term"].as_str().unwrap_or_default())))
}
