use serde_json::{json, Value};

use super::state::App;
use crate::constants::{DM_LENA, TEAM};
use crate::model::{channel_name, username};

impl App {
    pub(crate) fn post_channel_event(&self, post: &Value, mentions: Vec<&str>) {
        let channel_id = post["channel_id"].as_str().unwrap_or_default().to_string();
        let user_id = post["user_id"].as_str().unwrap_or_default();
        self.emit(
            "posted",
            json!({
                // The real server sends the post as a JSON *string*.
                "post": serde_json::to_string(post).unwrap(),
                "channel_type": if channel_id == DM_LENA { "D" } else { "O" },
                "channel_display_name": channel_name(&channel_id),
                "channel_name": channel_id,
                "sender_name": username(user_id),
                "team_id": TEAM,
                "set_online": true,
                "mentions": serde_json::to_string(&mentions).unwrap(),
            }),
            json!({ "channel_id": channel_id, "team_id": TEAM }),
        );
    }
}
