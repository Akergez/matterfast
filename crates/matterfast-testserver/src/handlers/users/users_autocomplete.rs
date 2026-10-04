use std::collections::HashMap;

use axum::extract::Query;
use axum::Json;
use serde_json::{json, Value};

use super::directory::directory;

/// `GET /users/autocomplete?name=…` — what the `@` list and the search box's
/// `from:` list ask. With no channel named there is no `out_of_channel`.
pub(crate) async fn users_autocomplete(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let name = q.get("name").map(String::as_str).unwrap_or_default();
    Json(json!({ "users": directory(name) }))
}
