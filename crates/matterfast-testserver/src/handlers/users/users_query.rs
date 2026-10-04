use std::collections::HashMap;

use axum::extract::Query;
use axum::Json;
use serde_json::{json, Value};

use crate::constants::{LENA, ME, MIKK, SARA};
use crate::model::user;

pub(crate) async fn users_query(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let _ = q;
    Json(json!([user(ME), user(LENA), user(MIKK), user(SARA)]))
}
