use gpui_kit::{px, App, FollowMode, ListOffset};
use mattermost_api::models::Post;

use super::chat_view::ChatView;
use crate::state::SharedState;
use crate::ui::chat::feed::feed_shows;
use crate::ui::chat::scroll_trace::scroll_trace_enabled;

impl ChatView {
    /// The row a post is drawn in, if it is in the feed at all.
    fn index_of(&self, post_id: &str) -> Option<usize> {
        self.items
            .borrow()
            .iter()
            .position(|item| item.post_id() == Some(post_id))
    }

    /// Scrolls the feed so a particular message is in view and briefly
    /// highlights it. Returns false when that message is not in the feed — the
    /// caller then knows it has to fetch further back first.
    pub fn scroll_to_post(&self, post_id: &str, cx: &mut App) -> bool {
        let Some(index) = self.index_of(post_id) else {
            return false;
        };
        if scroll_trace_enabled() {
            tracing::warn!(
                target: "matterfast::scroll",
                event = "programmatic-scroll-to",
                reason = "explicit-post-navigation",
                post_id,
                index,
                "scroll trace"
            );
        }
        self.scroll_near(index);
        *self.highlight.borrow_mut() = Some(post_id.to_string());
        cx.refresh_windows();

        let id = post_id.to_string();
        crate::runtime::after(std::time::Duration::from_secs(2), move |cx| {
            let Some(ui) = crate::ui::current(cx) else { return };
            let mut highlight = ui.chat.highlight.borrow_mut();
            if highlight.as_deref() == Some(id.as_str()) {
                *highlight = None;
                cx.refresh_windows();
            }
        });
        true
    }

    /// Puts a row a little way down from the top of the viewport, with what
    /// led up to it still visible above.
    pub(crate) fn scroll_near(&self, index: usize) {
        // Starting a row or two early is how "not jammed against the top
        // edge" is spelled in a list that scrolls by item.
        self.list.scroll_to(ListOffset {
            item_ix: index.saturating_sub(2),
            offset_in_item: px(0.),
        });
    }

    /// The message the reader is at, as (channel, post id). The post is `None`
    /// at the live bottom. A post id rather than pixels, so the position
    /// survives fonts, window width and late media layout.
    pub fn current_anchor(&self, _cx: &App) -> Option<(String, Option<String>)> {
        let channel_id = self.showing.borrow().clone()?;
        if self.list.is_following_tail() {
            return Some((channel_id, None));
        }
        let top = self.list.logical_scroll_top().item_ix;
        let items = self.items.borrow();
        // A couple of rows in from the top: the first row may be mostly
        // scrolled away, and a day separator is not a message.
        let post_id = items
            .iter()
            .skip(top + 2)
            .chain(items.iter().skip(top))
            .find_map(|item| item.post_id().map(str::to_string));
        Some((channel_id, post_id))
    }

    /// Goes back to a saved message. This is an explicit navigation, not
    /// pagination compensation.
    pub fn restore_anchor(&self, post_id: &str, cx: &mut App) -> bool {
        let Some(index) = self.index_of(post_id) else {
            return false;
        };
        if scroll_trace_enabled() {
            tracing::warn!(
                target: "matterfast::scroll",
                event = "programmatic-scroll-to",
                reason = "restore-saved-anchor",
                post_id,
                index,
                "scroll trace"
            );
        }
        self.scroll_near(index);
        cx.refresh_windows();
        true
    }

    /// Puts the feed on its newest row and keeps it there.
    pub(crate) fn scroll_to_newest(&self, reason: &str) {
        if scroll_trace_enabled() {
            tracing::warn!(
                target: "matterfast::scroll",
                event = "programmatic-scroll-to",
                reason,
                "scroll trace"
            );
        }
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
    }

    /// Puts the feed at the bottom because *you* posted. Following only when
    /// the reader was already at the live edge is the right rule for someone
    /// else's message and the wrong one for your own: every other client
    /// shows you what you just sent, wherever you had been reading.
    ///
    /// Does nothing when the post is not in this feed (a reply CRT keeps in
    /// its thread, or a thread whose root lives in another channel), so
    /// answering in the thread panel does not drag the channel behind it.
    pub fn follow_own_post(&self, post: &Post, state: &SharedState, cx: &mut App) {
        if !feed_shows(
            self.showing.borrow().as_deref(),
            post,
            state.borrow().crt_enabled,
        ) {
            return;
        }
        self.scroll_to_newest("own-message-sent");
        cx.refresh_windows();
    }
}
