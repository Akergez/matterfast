use serde_json::{json, Value};

use super::channel_name::channel_name;
use super::filler::filler_index;
use crate::clock::now;
use crate::constants::{DEV, DM_LENA, LENA, ME, SARA, TEAM};

pub(crate) fn channel(channel_id: &str) -> Value {
    if let Some(n) = filler_index(channel_id) {
        return json!({
            "id": channel_id,
            "create_at": 1_700_000_000_000i64,
            "update_at": 1_700_000_000_000i64,
            "delete_at": 0,
            "team_id": TEAM,
            "type": "O",
            "display_name": channel_name(channel_id),
            "name": format!("channel-{n}"),
            "header": "",
            "purpose": "",
            "last_post_at": now(),
            "total_msg_count": 6,
            "total_msg_count_root": 6,
            "creator_id": SARA,
        });
    }
    let (name, display, kind, total) = match channel_id {
        DEV => ("development", "Development", "O", 12),
        DM_LENA => (
            // A DM's name is the two ids joined by a double underscore.
            Box::leak(format!("{ME}__{LENA}").into_boxed_str()) as &str,
            "",
            "D",
            3,
        ),
        _ => ("general", "General", "O", 6),
    };
    json!({
        "id": channel_id,
        "create_at": 1_700_000_000_000i64,
        "update_at": 1_700_000_000_000i64,
        "delete_at": 0,
        "team_id": if kind == "D" { "" } else { TEAM },
        "type": kind,
        "display_name": display,
        "name": name,
        "header": if channel_id == DEV { "Client work · protocol notes pinned" } else { "" },
        "purpose": "",
        "last_post_at": now(),
        "total_msg_count": total,
        "total_msg_count_root": total,
        "creator_id": SARA,
    })
}
