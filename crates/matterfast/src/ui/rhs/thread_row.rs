use std::rc::Rc;

use gpui_kit::SharedString;
use mattermost_api::models::Post;

/// One row of an open thread.
#[derive(Clone)]
pub(super) enum ThreadRow {
    /// "3 replies", under the root. It is where Mattermost puts the reply
    /// count too, and it makes the thread readable at a glance.
    Divider(String),
    Post {
        post: Rc<Post>,
        grouped: bool,
        body: SharedString,
    },
}

impl ThreadRow {
    pub(super) fn key(&self) -> String {
        match self {
            ThreadRow::Divider(_) => "divider".to_string(),
            ThreadRow::Post { post, .. } => format!("post:{}", post.id),
        }
    }
}
