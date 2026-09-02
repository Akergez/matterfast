//! Channel, membership, unread, stats and sidebar-category models.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{Millis, StringMap};

/// `server/public/model/channel.go`. Modelled with an `Other` arm so a server
/// that grows a new channel type does not break deserialization of the feed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelType {
    #[default]
    #[serde(rename = "O")]
    Open,
    #[serde(rename = "P")]
    Private,
    #[serde(rename = "D")]
    Direct,
    #[serde(rename = "G")]
    Group,
    #[serde(rename = "S")]
    Space,
    #[serde(untagged)]
    Other(String),
}

impl ChannelType {
    pub fn is_dm_or_gm(&self) -> bool {
        matches!(self, ChannelType::Direct | ChannelType::Group)
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Channel {
    pub id: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub r#type: ChannelType,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub last_post_at: Millis,
    #[serde(default)]
    pub total_msg_count: i64,
    #[serde(default)]
    pub total_msg_count_root: i64,
    #[serde(default)]
    pub last_root_post_at: Millis,
    #[serde(default)]
    pub creator_id: String,
    #[serde(default)]
    pub group_constrained: Option<bool>,
    #[serde(default)]
    pub shared: Option<bool>,
}

impl Channel {
    pub fn is_archived(&self) -> bool {
        self.delete_at != 0
    }

    /// For a DM the `name` is `"<userA>__<userB>"` with ids sorted; this returns
    /// the id that is *not* `me`.
    pub fn dm_teammate_id(&self, me: &str) -> Option<&str> {
        if self.r#type != ChannelType::Direct {
            return None;
        }
        let (a, b) = self.name.split_once("__")?;
        if a == me {
            Some(b)
        } else if b == me {
            Some(a)
        } else {
            None
        }
    }
}

/// `server/public/model/channel_member.go`.
///
/// `last_viewed_at` and `last_update_at` are `Option` on purpose: when you fetch
/// *another* user's membership the server sanitizes them to `-1` and its custom
/// `MarshalJSON` then omits the fields entirely.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChannelMember {
    pub channel_id: String,
    pub user_id: String,
    #[serde(default)]
    pub roles: String,
    #[serde(default)]
    pub last_viewed_at: Option<Millis>,
    #[serde(default)]
    pub msg_count: i64,
    #[serde(default)]
    pub mention_count: i64,
    #[serde(default)]
    pub mention_count_root: i64,
    #[serde(default)]
    pub urgent_mention_count: i64,
    #[serde(default)]
    pub msg_count_root: i64,
    #[serde(default)]
    pub notify_props: StringMap,
    #[serde(default)]
    pub last_update_at: Option<Millis>,
    #[serde(default)]
    pub scheme_admin: bool,
}

impl ChannelMember {
    /// A channel is muted when `notify_props.mark_unread == "mention"`.
    pub fn is_muted(&self) -> bool {
        self.notify_props.get("mark_unread").map(String::as_str) == Some("mention")
    }
}

/// The unread state of one channel, as the official clients compute it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnreadState {
    pub messages: i64,
    pub mentions: i64,
    pub urgent: bool,
    pub muted: bool,
}

impl UnreadState {
    /// `showUnread = mentions > 0 || (!muted && messages > 0)`
    /// — `packages/mattermost-redux/src/utils/channel_utils.ts:371`.
    pub fn is_unread(&self) -> bool {
        self.mentions > 0 || (!self.muted && self.messages > 0)
    }

    /// Computes unread counts from a channel and your membership.
    ///
    /// Under collapsed reply threads (CRT) the `_root` counters are the ones
    /// that matter, because replies are accounted to threads instead.
    pub fn compute(channel: &Channel, member: &ChannelMember, crt_enabled: bool) -> Self {
        let (messages, mentions) = if crt_enabled {
            (
                channel.total_msg_count_root - member.msg_count_root,
                member.mention_count_root,
            )
        } else {
            (
                channel.total_msg_count - member.msg_count,
                member.mention_count,
            )
        };
        UnreadState {
            messages: messages.max(0),
            mentions,
            urgent: member.urgent_mention_count > 0,
            muted: member.is_muted(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelUnread {
    #[serde(default)]
    pub team_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub msg_count: i64,
    #[serde(default)]
    pub mention_count: i64,
    #[serde(default)]
    pub mention_count_root: i64,
    #[serde(default)]
    pub urgent_mention_count: i64,
    #[serde(default)]
    pub msg_count_root: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelStats {
    pub channel_id: String,
    #[serde(default)]
    pub member_count: i64,
    #[serde(default)]
    pub guest_count: i64,
    /// Note the missing underscore — the server really does serialize it as
    /// `pinnedpost_count`.
    #[serde(default, rename = "pinnedpost_count")]
    pub pinned_post_count: i64,
    #[serde(default)]
    pub files_count: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ChannelView {
    pub channel_id: String,
    #[serde(default)]
    pub prev_channel_id: String,
    pub collapsed_threads_supported: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelViewResponse {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_viewed_at_times: HashMap<String, Millis>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryType {
    #[default]
    Channels,
    DirectMessages,
    Favorites,
    Custom,
    Managed,
    #[serde(untagged)]
    Other(String),
}

/// `GET /users/{me}/teams/{team}/channels/categories` returns
/// [`OrderedSidebarCategories`]; each entry flattens the embedded
/// `SidebarCategory` struct.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SidebarCategory {
    pub id: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub sort_order: i64,
    /// `""` | `manual` | `recent` | `alpha`
    #[serde(default)]
    pub sorting: String,
    #[serde(default)]
    pub r#type: CategoryType,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub collapsed: bool,
    /// The Go field is `Channels`; the JSON key is `channel_ids`.
    #[serde(default)]
    pub channel_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OrderedSidebarCategories {
    #[serde(default)]
    pub categories: Vec<SidebarCategory>,
    #[serde(default)]
    pub order: Vec<String>,
}

/// `server/public/model/channel_bookmark.go` — the pinned links/files strip at
/// the top of a channel. **Server 9.4+**: older servers answer 404 for the
/// whole `/channels/{channel}/bookmarks` route, which is how you feature-detect
/// it, since the client config carries no flag for it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChannelBookmark {
    pub id: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub display_name: String,
    /// Set for `type == "link"`, empty for a file bookmark.
    #[serde(default)]
    pub link_url: String,
    /// Set for `type == "file"`, empty for a link bookmark.
    #[serde(default)]
    pub file_id: String,
    /// `"link"` | `"file"`
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub sort_order: i64,
}
