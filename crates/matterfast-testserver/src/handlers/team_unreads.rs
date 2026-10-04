use axum::Json;
use serde_json::{json, Value};

use crate::constants::TEAM;

pub(crate) async fn team_unreads() -> Json<Value> {
    Json(json!([{
        "team_id": TEAM, "msg_count": 5, "mention_count": 1,
        "mention_count_root": 1, "msg_count_root": 5,
        "thread_count": 1, "thread_mention_count": 1, "thread_urgent_mention_count": 0,
    }]))
}
