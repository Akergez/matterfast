use axum::Json;
use serde_json::Value;

use crate::constants::{LENA, ME, MIKK, OLGA, SARA};
use crate::model::user;

/// `POST /users/usernames` — how a client resolves a mention of somebody it
/// has not met.
pub(crate) async fn users_by_usernames(Json(names): Json<Vec<String>>) -> Json<Value> {
    let found = [ME, LENA, MIKK, SARA, OLGA]
        .into_iter()
        .map(user)
        .filter(|u| names.iter().any(|name| u["username"] == name.as_str()))
        .collect();
    Json(Value::Array(found))
}
