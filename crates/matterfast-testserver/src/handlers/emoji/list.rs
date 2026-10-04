use std::collections::HashMap;

use axum::extract::Query;
use axum::Json;
use serde_json::Value;

use super::names::EMOJI;
use super::record::emoji;

pub(crate) async fn emoji_list(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    // One page holds them all; any later page is empty.
    let first = q.get("page").is_none_or(|page| page == "0");
    Json(Value::Array(if first {
        EMOJI.iter().map(|name| emoji(name)).collect()
    } else {
        Vec::new()
    }))
}
