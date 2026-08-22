//! User, status and custom-status models.
//!
//! Field names mirror `server/public/model/user.go`, `status.go`,
//! `custom_status.go` verbatim.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::Millis;

pub type StringMap = HashMap<String, String>;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub auth_service: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub position: String,
    /// Space-separated, e.g. `"system_user system_admin"`. Use [`User::has_role`].
    #[serde(default)]
    pub roles: String,
    #[serde(default)]
    pub props: StringMap,
    #[serde(default)]
    pub notify_props: StringMap,
    #[serde(default)]
    pub last_picture_update: Millis,
    #[serde(default)]
    pub locale: String,
    /// Keys: `useAutomaticTimezone` (the string `"true"`/`"false"`),
    /// `automaticTimezone`, `manualTimezone`.
    #[serde(default)]
    pub timezone: StringMap,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub bot_description: String,
    #[serde(default)]
    pub last_activity_at: Millis,
}

impl User {
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.split_whitespace().any(|r| r == role)
    }

    pub fn is_deleted(&self) -> bool {
        self.delete_at != 0
    }

    /// The custom status lives in `props["customStatus"]` as a *JSON-encoded
    /// string*, not as a nested object.
    pub fn custom_status(&self) -> Option<CustomStatus> {
        let raw = self.props.get("customStatus")?;
        serde_json::from_str(raw).ok()
    }

    /// Best display name for the given teammate-name-display preference value
    /// (`username` | `nickname_full_name` | `full_name`).
    pub fn display_name(&self, setting: &str) -> String {
        let full = format!("{} {}", self.first_name, self.last_name);
        let full = full.trim();
        match setting {
            "nickname_full_name" if !self.nickname.is_empty() => self.nickname.clone(),
            "nickname_full_name" | "full_name" if !full.is_empty() => full.to_string(),
            _ => self.username.clone(),
        }
    }
}

/// `server/public/model/custom_status.go`.
///
/// Note `expires_at` is RFC3339, **not** milliseconds — it is the only
/// timestamp in the API shaped that way besides `Status::dnd_end_time`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CustomStatus {
    #[serde(default)]
    pub emoji: String,
    #[serde(default)]
    pub text: String,
    /// `thirty_minutes` | `one_hour` | `four_hours` | `today` | `this_week` |
    /// `date_and_time`
    #[serde(default)]
    pub duration: String,
    #[serde(default)]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Presence {
    Online,
    Away,
    #[default]
    Offline,
    Dnd,
    OutOfOffice,
}

impl Presence {
    pub fn as_str(self) -> &'static str {
        match self {
            Presence::Online => "online",
            Presence::Away => "away",
            Presence::Offline => "offline",
            Presence::Dnd => "dnd",
            Presence::OutOfOffice => "ooo",
        }
    }
}

impl From<&str> for Presence {
    fn from(s: &str) -> Self {
        match s {
            "online" => Presence::Online,
            "away" => Presence::Away,
            "dnd" => Presence::Dnd,
            "ooo" => Presence::OutOfOffice,
            _ => Presence::Offline,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Status {
    pub user_id: String,
    /// `online` | `away` | `offline` | `dnd` | `ooo`
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub last_activity_at: Millis,
    /// **Seconds**, not milliseconds — the one exception in the whole API.
    #[serde(default)]
    pub dnd_end_time: i64,
}

impl Status {
    pub fn presence(&self) -> Presence {
        Presence::from(self.status.as_str())
    }
}

/// `GET /users/autocomplete`
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserAutocomplete {
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub out_of_channel: Vec<User>,
}
