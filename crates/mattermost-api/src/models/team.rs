//! Team, membership and unread models (`server/public/model/team*.go`).

use serde::{Deserialize, Serialize};

use super::Millis;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Team {
    pub id: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// `"O"` open | `"I"` invite-only
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub allow_open_invite: bool,
    #[serde(default)]
    pub last_team_icon_update: Millis,
    #[serde(default)]
    pub group_constrained: Option<bool>,
}

/// Note: `create_at` is tagged `json:"-"` server-side and never appears.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TeamMember {
    pub team_id: String,
    pub user_id: String,
    #[serde(default)]
    pub roles: String,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub scheme_guest: bool,
    #[serde(default)]
    pub scheme_user: bool,
    #[serde(default)]
    pub scheme_admin: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TeamUnread {
    pub team_id: String,
    #[serde(default)]
    pub msg_count: i64,
    #[serde(default)]
    pub mention_count: i64,
    #[serde(default)]
    pub mention_count_root: i64,
    #[serde(default)]
    pub msg_count_root: i64,
    #[serde(default)]
    pub thread_count: i64,
    #[serde(default)]
    pub thread_mention_count: i64,
    #[serde(default)]
    pub thread_urgent_mention_count: i64,
}
