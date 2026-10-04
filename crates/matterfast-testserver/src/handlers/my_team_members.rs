use axum::Json;
use serde_json::{json, Value};

use crate::constants::{ME, TEAM};

pub(crate) async fn my_team_members() -> Json<Value> {
    Json(json!([{
        "team_id": TEAM, "user_id": ME, "roles": "team_user",
        "delete_at": 0, "scheme_user": true, "scheme_admin": false,
    }]))
}
