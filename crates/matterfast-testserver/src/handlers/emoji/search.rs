use axum::Json;
use serde_json::Value;

use super::names::EMOJI;
use super::record::emoji;

pub(crate) async fn emoji_search(Json(body): Json<Value>) -> Json<Value> {
    let term = body["term"].as_str().unwrap_or_default().to_lowercase();
    Json(Value::Array(
        EMOJI
            .iter()
            .filter(|name| name.contains(&term))
            .map(|name| emoji(name))
            .collect(),
    ))
}
