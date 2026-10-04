use serde_json::{json, Value};

use super::channel::channel;
use crate::clock::now;
use crate::constants::ME;

pub(crate) fn membership(channel_id: &str, unread: i64, mentions: i64) -> Value {
    let total = channel(channel_id)["total_msg_count"].as_i64().unwrap_or(0);
    json!({
        "channel_id": channel_id,
        "user_id": ME,
        "roles": "channel_user",
        "last_viewed_at": now(),
        "msg_count": total - unread,
        "msg_count_root": total - unread,
        "mention_count": mentions,
        "mention_count_root": mentions,
        "urgent_mention_count": 0,
        "notify_props": {},
        "last_update_at": now(),
        "scheme_admin": false,
    })
}
