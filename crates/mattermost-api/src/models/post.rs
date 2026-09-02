//! Post, post metadata and post-list models (`server/public/model/post*.go`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{file::FileInfo, Millis};

/// Free-form `props` bag.
pub type Props = HashMap<String, serde_json::Value>;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Post {
    pub id: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    /// Non-zero once the post has been edited. Do **not** use `update_at` for
    /// this — it also bumps on reactions and pins.
    #[serde(default)]
    pub edit_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub channel_id: String,
    /// `""` for a root post, otherwise the root post's id.
    #[serde(default)]
    pub root_id: String,
    #[serde(default)]
    pub original_id: String,
    #[serde(default)]
    pub message: String,
    /// The user's raw text when the server rewrote `message` for presentation
    /// (image proxy). Prefer this when populating an edit box.
    #[serde(default)]
    pub message_source: String,
    /// `""` for a normal post; `system_*` for system messages.
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub props: Props,
    #[serde(default)]
    pub hashtags: String,
    #[serde(default)]
    pub file_ids: Vec<String>,
    #[serde(default)]
    pub pending_post_id: String,
    #[serde(default)]
    pub has_reactions: bool,
    #[serde(default)]
    pub reply_count: i64,
    #[serde(default)]
    pub last_reply_at: Millis,
    #[serde(default)]
    pub is_following: Option<bool>,
    #[serde(default)]
    pub metadata: Option<PostMetadata>,
}

impl Post {
    pub fn is_reply(&self) -> bool {
        !self.root_id.is_empty()
    }

    pub fn is_deleted(&self) -> bool {
        self.delete_at != 0
    }

    pub fn is_edited(&self) -> bool {
        self.edit_at != 0
    }

    /// System messages (joins, leaves, header changes …) are rendered
    /// differently and are excluded from unread counts.
    pub fn is_system(&self) -> bool {
        self.r#type.starts_with("system_")
    }

    /// `/me` messages.
    pub fn is_emote(&self) -> bool {
        self.r#type == "me"
    }

    /// The id a thread groups under: the root id for replies, own id otherwise.
    pub fn thread_root(&self) -> &str {
        if self.root_id.is_empty() {
            &self.id
        } else {
            &self.root_id
        }
    }

    /// Text to edit — `message_source` when the server rewrote the message.
    pub fn source_text(&self) -> &str {
        if self.message_source.is_empty() {
            &self.message
        } else {
            &self.message_source
        }
    }

    /// True for a post the client created optimistically and the server has
    /// not echoed back yet: we set the id to the pending id so the echo can
    /// replace it in place.
    pub fn is_pending(&self) -> bool {
        !self.pending_post_id.is_empty() && self.pending_post_id == self.id
    }

    pub fn from_webhook(&self) -> bool {
        self.props.get("from_webhook").and_then(|v| v.as_str()) == Some("true")
    }

    /// Webhooks and bots may override the displayed username.
    pub fn override_username(&self) -> Option<&str> {
        self.props.get("override_username").and_then(|v| v.as_str())
    }

    pub fn priority(&self) -> Option<&PostPriority> {
        self.metadata.as_ref()?.priority.as_ref()
    }

    pub fn files(&self) -> &[FileInfo] {
        self.metadata
            .as_ref()
            .map(|m| m.files.as_slice())
            .unwrap_or(&[])
    }

    pub fn reactions(&self) -> &[Reaction] {
        self.metadata
            .as_ref()
            .map(|m| m.reactions.as_slice())
            .unwrap_or(&[])
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostMetadata {
    #[serde(default)]
    pub embeds: Vec<PostEmbed>,
    #[serde(default)]
    pub emojis: Vec<Emoji>,
    #[serde(default)]
    pub files: Vec<FileInfo>,
    #[serde(default)]
    pub images: HashMap<String, PostImage>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub priority: Option<PostPriority>,
    #[serde(default)]
    pub acknowledgements: Vec<PostAcknowledgement>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostEmbed {
    /// `image` | `message_attachment` | `opengraph` | `link` | `permalink` |
    /// `boards`
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct PostImage {
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub frame_count: i32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostPriority {
    /// `""` | `"important"` | `"urgent"`
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub requested_ack: Option<bool>,
    #[serde(default)]
    pub persistent_notifications: Option<bool>,
}

impl PostPriority {
    pub fn is_urgent(&self) -> bool {
        self.priority.as_deref() == Some("urgent")
    }
    pub fn is_important(&self) -> bool {
        self.priority.as_deref() == Some("important")
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostAcknowledgement {
    pub user_id: String,
    pub post_id: String,
    #[serde(default)]
    pub acknowledged_at: Millis,
    #[serde(default)]
    pub channel_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Emoji {
    pub id: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub creator_id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Reaction {
    pub user_id: String,
    pub post_id: String,
    pub emoji_name: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub channel_id: String,
}

/// The shape every feed endpoint returns.
///
/// `order` is **newest-first**; `posts` is keyed by id.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PostList {
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub posts: HashMap<String, Post>,
    #[serde(default)]
    pub next_post_id: String,
    #[serde(default)]
    pub prev_post_id: String,
    #[serde(default)]
    pub has_next: Option<bool>,
    #[serde(default)]
    pub first_inaccessible_post_time: Millis,
}

impl PostList {
    /// `next_post_id == ""` means this page reaches the newest post in the
    /// channel — the block is `recent` in webapp terms.
    pub fn is_recent(&self) -> bool {
        self.next_post_id.is_empty()
    }

    /// `prev_post_id == ""` means this page reaches the beginning of the
    /// channel — the block is `oldest`.
    pub fn is_oldest(&self) -> bool {
        self.prev_post_id.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Posts in `order` (newest first), skipping ids the map does not carry.
    pub fn ordered(&self) -> impl Iterator<Item = &Post> {
        self.order.iter().filter_map(|id| self.posts.get(id))
    }

    /// Oldest-first, which is the order a chat feed renders in.
    pub fn chronological(&self) -> Vec<&Post> {
        let mut v: Vec<&Post> = self.ordered().collect();
        v.reverse();
        v
    }

    pub fn newest(&self) -> Option<&Post> {
        self.order.first().and_then(|id| self.posts.get(id))
    }

    pub fn oldest_post(&self) -> Option<&Post> {
        self.order.last().and_then(|id| self.posts.get(id))
    }
}

/// One entry of the collapsed-reply-threads inbox
/// (`GET /users/{me}/teams/{team}/threads`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserThread {
    /// The root post's id — a thread is identified by its root.
    pub id: String,
    #[serde(default)]
    pub reply_count: i64,
    #[serde(default)]
    pub last_reply_at: Millis,
    #[serde(default)]
    pub last_viewed_at: Millis,
    #[serde(default)]
    pub unread_replies: i64,
    #[serde(default)]
    pub unread_mentions: i64,
    #[serde(default)]
    pub is_following: bool,
    #[serde(default)]
    pub post: Post,
    #[serde(default)]
    pub participants: Vec<Participant>,
}

impl UserThread {
    pub fn is_unread(&self) -> bool {
        self.unread_replies > 0 || self.unread_mentions > 0
    }
}

/// Thread participants come back as full users on some servers and as bare
/// `{id}` stubs on others, so accept either.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Participant {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
}

/// `GET /users/{me}/teams/{team}/threads`
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserThreads {
    #[serde(default)]
    pub threads: Vec<UserThread>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub total_unread_threads: i64,
    #[serde(default)]
    pub total_unread_mentions: i64,
}

/// `POST /teams/{team}/posts/search`
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PostSearchResults {
    #[serde(flatten)]
    pub posts: PostList,
    #[serde(default)]
    pub matches: HashMap<String, Vec<String>>,
}

/// `server/public/model/scheduled_post.go` — a [`Post`] plus the time to send
/// it. The server stores it separately and only creates the real post at
/// `scheduled_at`.
///
/// `error_code` is filled in when a send *later* failed (`channel_archived`,
/// `no_channel_permission`, `unknown`): the scheduled post stays in the list
/// with the error on it rather than disappearing, so the client can show why.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ScheduledPost {
    /// The post to send — same wire fields, flattened into this object.
    #[serde(flatten)]
    pub post: Post,
    #[serde(default)]
    pub scheduled_at: Millis,
    #[serde(default)]
    pub error_code: String,
}
