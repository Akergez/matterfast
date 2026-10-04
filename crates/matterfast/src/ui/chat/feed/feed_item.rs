use std::rc::Rc;

use gpui_kit::SharedString;
use mattermost_api::models::Post;

/// One row of the feed.
///
/// Built once per redraw of the conversation rather than once per frame: the
/// expensive parts — turning a message into Markdown, deciding who it groups
/// with — are done here, and drawing a row is then only laying it out.
#[derive(Clone)]
pub enum FeedItem {
    /// "This is the beginning of …".
    Start(String),
    Empty,
    Day(String),
    Unread,
    /// A run of joins, leaves and the like, as lines of Markdown.
    System {
        key: String,
        lines: Rc<Vec<SharedString>>,
    },
    Post {
        post: Rc<Post>,
        grouped: bool,
        /// The message as Markdown, ready for the text view.
        body: SharedString,
    },
}

impl FeedItem {
    pub fn post_id(&self) -> Option<&str> {
        match self {
            FeedItem::Post { post, .. } => Some(&post.id),
            _ => None,
        }
    }

    /// Which row this is, as opposed to what it currently says. Two feeds are
    /// compared by these to find what was added and what was taken away; a
    /// row whose key survives keeps its place in the list, and with it the
    /// reader's scroll position.
    pub(crate) fn key(&self) -> String {
        match self {
            FeedItem::Start(_) => "start".to_string(),
            FeedItem::Empty => "empty".to_string(),
            FeedItem::Day(day) => format!("day:{day}"),
            FeedItem::Unread => "unread".to_string(),
            FeedItem::System { key, .. } => format!("system:{key}"),
            FeedItem::Post { post, .. } => format!("post:{}", post.id),
        }
    }
}
