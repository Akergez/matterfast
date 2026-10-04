use axum::Json;
use serde_json::{json, Value};

use crate::constants::TEAM;

pub(crate) async fn my_teams() -> Json<Value> {
    Json(json!([{
        "id": TEAM,
        "create_at": 1_700_000_000_000i64,
        "update_at": 1_700_000_000_000i64,
        "delete_at": 0,
        "display_name": "Core Platform",
        "name": "core",
        "description": "",
        "type": "O",
        "allow_open_invite": false,
    }]))
}
