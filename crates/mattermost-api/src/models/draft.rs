//! Server-side drafts (`server/public/model/draft.go`).
//!
//! A draft is what you typed but have not sent. The server stores one per
//! (user, channel, root) so the same unfinished message follows you between
//! devices — which is the only reason to involve the server at all.

use serde::{Deserialize, Serialize};

use super::{post::Props, Millis};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Draft {
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub user_id: String,
    pub channel_id: String,
    /// `""` for a channel draft, otherwise the root post of the thread it
    /// belongs to.
    #[serde(default)]
    pub root_id: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub file_ids: Vec<String>,
    #[serde(default)]
    pub props: Props,
}

impl Draft {
    pub fn new(channel_id: &str, root_id: &str, message: &str) -> Draft {
        Draft {
            channel_id: channel_id.to_string(),
            root_id: root_id.to_string(),
            message: message.to_string(),
            ..Default::default()
        }
    }

    /// Where this draft belongs: the thread root if it has one, else the
    /// channel. Drafts are keyed by this pair everywhere.
    pub fn key(&self) -> (String, String) {
        (self.channel_id.clone(), self.root_id.clone())
    }
}
