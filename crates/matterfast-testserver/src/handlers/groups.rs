use std::collections::HashMap;

use axum::extract::{Path, Query};
use axum::Json;
use serde_json::{json, Value};

use crate::constants::{LENA, MIKK, OLGA, SARA};
use crate::model::user;

/// `GET /groups` — the groups that can be mentioned. One of them has no
/// mention name, as a directory group nobody has set up does.
pub(crate) async fn groups(Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let first = q.get("page").is_none_or(|page| page == "0");
    Json(if first {
        json!([
            {"id": "group-backend", "name": "backend", "display_name": "Backend team",
             "source": "custom", "allow_reference": true, "member_count": 3, "delete_at": 0},
            {"id": "group-oncall", "name": "on-call", "display_name": "On call",
             "source": "custom", "allow_reference": true, "member_count": 1, "delete_at": 0},
            {"id": "group-ldap", "name": null, "display_name": "Directory only",
             "source": "ldap", "allow_reference": true, "member_count": 2, "delete_at": 0},
        ])
    } else {
        json!([])
    })
}

/// `GET /groups/{id}/members` — who a group mention reaches.
pub(crate) async fn group_members(Path(id): Path<String>) -> Json<Value> {
    let members: Vec<Value> = match id.as_str() {
        "group-backend" => vec![user(LENA), user(MIKK), user(OLGA)],
        "group-oncall" => vec![user(SARA)],
        _ => Vec::new(),
    };
    Json(json!({ "total_member_count": members.len(), "members": members }))
}
