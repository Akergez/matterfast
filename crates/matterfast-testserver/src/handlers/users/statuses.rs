use axum::Json;
use serde_json::{json, Value};

use crate::clock::now;
use crate::constants::{LENA, MIKK, SARA};

pub(crate) async fn statuses(Json(ids): Json<Vec<String>>) -> Json<Value> {
    Json(Value::Array(
        ids.iter()
            .map(|i| {
                let status = match i.as_str() {
                    LENA => "online",
                    MIKK => "away",
                    SARA => "dnd",
                    _ => "online",
                };
                json!({"user_id": i, "status": status, "manual": false,
                       "last_activity_at": now(), "dnd_end_time": 0})
            })
            .collect(),
    ))
}
