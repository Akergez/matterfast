//! Serde models mirroring `server/public/model/*.go`.
//!
//! Conventions worth remembering while reading these:
//!
//! * every `*_at` field is Unix **milliseconds** ([`Millis`]), and `0` means
//!   "unset" — the two exceptions are [`user::Status::dnd_end_time`]
//!   (seconds) and [`user::CustomStatus::expires_at`] (RFC3339);
//! * `roles` fields are **space-separated strings**, not arrays;
//! * anything the server calls a "props" or "notify_props" bag is a
//!   `map[string]string` or `map[string]any` and is modelled as such.

pub mod channel;
pub mod file;
pub mod post;
pub mod team;
pub mod user;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Unix milliseconds. `0` means unset.
pub type Millis = i64;

pub type StringMap = HashMap<String, String>;

pub use channel::{
    CategoryType, Channel, ChannelMember, ChannelStats, ChannelType, ChannelUnread, ChannelView,
    ChannelViewResponse, OrderedSidebarCategories, SidebarCategory, UnreadState,
};
pub use file::{FileInfo, FileUploadResponse};
pub use post::{
    Emoji, Participant, Post, PostAcknowledgement, PostEmbed, PostList, PostMetadata, PostPriority,
    PostSearchResults, Reaction, UserThread, UserThreads,
};
pub use team::{Team, TeamMember, TeamUnread};
pub use user::{CustomStatus, Presence, Status, User, UserAutocomplete};

/// `server/public/model/preference.go`. `value` is *always* a string, even for
/// booleans (`"true"`) and JSON blobs.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Preference {
    pub user_id: String,
    pub category: String,
    pub name: String,
    pub value: String,
}

impl Preference {
    /// The key the webapp uses in its flat preference map.
    pub fn key(&self) -> String {
        format!("{}--{}", self.category, self.name)
    }

    pub fn as_bool(&self) -> bool {
        self.value == "true"
    }
}

/// Well-known preference categories.
pub mod preference_category {
    pub const DIRECT_CHANNEL_SHOW: &str = "direct_channel_show";
    pub const GROUP_CHANNEL_SHOW: &str = "group_channel_show";
    pub const FAVORITE_CHANNEL: &str = "favorite_channel";
    pub const FLAGGED_POST: &str = "flagged_post";
    pub const DISPLAY_SETTINGS: &str = "display_settings";
    pub const ADVANCED_SETTINGS: &str = "advanced_settings";
    pub const SIDEBAR_SETTINGS: &str = "sidebar_settings";
    pub const THEME: &str = "theme";
    pub const NOTIFICATIONS: &str = "notifications";
    pub const TEAMS_ORDER: &str = "teams_order";
}

/// `GET /api/v4/config/client?format=old` — a flat `map[string]string` of a few
/// hundred keys. Kept untyped on purpose; it changes every release.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct ClientConfig(pub StringMap);

impl ClientConfig {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn bool(&self, key: &str) -> bool {
        self.get(key) == Some("true")
    }

    pub fn version(&self) -> &str {
        self.get("Version").unwrap_or_default()
    }

    /// `disabled` | `default_on` | `default_off` | `always_on`
    pub fn collapsed_threads(&self) -> &str {
        self.get("CollapsedThreads").unwrap_or("disabled")
    }
}

/// Resolves whether collapsed reply threads are on, given the server setting
/// and the user's `display_settings/collapsed_reply_threads` preference.
///
/// Mirrors `processIsCRTEnabled` in mattermost-mobile.
pub fn is_crt_enabled(config: &ClientConfig, user_pref: Option<&str>) -> bool {
    match config.collapsed_threads() {
        "always_on" => true,
        "disabled" => false,
        setting => match user_pref {
            Some("on") => true,
            Some("off") => false,
            _ => setting == "default_on",
        },
    }
}
