use gpui_kit::SharedString;
use mattermost_api::models::Millis;

use super::target::Target;

/// One entry in the inbox, with everything it draws worked out already.
#[derive(Clone)]
pub(super) struct InboxRow {
    pub(super) user_id: String,
    pub(super) author: String,
    pub(super) channel: String,
    pub(super) preview: SharedString,
    pub(super) at: Millis,
    /// (replies, unread replies, unread mentions), for a followed thread.
    pub(super) counts: Option<(i64, i64, i64)>,
    /// Whether the reader saved this message: said on the row, because that
    /// and not its age is why it is here.
    pub(super) saved: bool,
    pub(super) target: Target,
}
