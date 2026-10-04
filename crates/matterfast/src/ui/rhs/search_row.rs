use std::rc::Rc;

use gpui_kit::SharedString;
use mattermost_api::models::Post;

/// A search hit: the message, and which channel it came from.
#[derive(Clone)]
pub(super) struct SearchRow {
    pub(super) channel: String,
    pub(super) post: Rc<Post>,
    pub(super) body: SharedString,
}
