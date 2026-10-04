use serde_json::{json, Value};

use crate::constants::ME;

pub(super) fn emoji(name: &str) -> Value {
    json!({"id": format!("emoji-{name}"), "name": name, "creator_id": ME,
           "create_at": 1, "update_at": 1, "delete_at": 0})
}
