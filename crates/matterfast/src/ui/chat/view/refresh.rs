use gpui_kit::App;

use super::chat_view::ChatView;
use crate::state::SharedState;
use crate::ui::chat::feed::{build_feed_items, splice_plan, FeedItem};
use crate::ui::chat::scroll_trace::scroll_trace_enabled;

impl ChatView {
    /// Redraws the feed for the current channel.
    ///
    /// The rows are rebuilt from the state and compared with what the list
    /// held: whatever is common at either end keeps its place, so an older
    /// page arriving, a new message, an edit and a reaction are all the same
    /// operation, and none of them moves a reader who is not at the bottom.
    pub fn refresh(&self, state: &SharedState, cx: &mut App) {
        let st = state.borrow();
        let channel = st
            .current_channel
            .clone()
            .and_then(|id| st.channel(&id).cloned());
        let Some(channel) = channel else {
            drop(st);
            if self.showing.borrow_mut().take().is_some() {
                self.items.borrow_mut().clear();
                self.list.reset(0);
            }
            crate::ui::refresh(cx);
            return;
        };
        let channel_id = channel.id.clone();

        // Only when something is actually unread: the line is a landmark, not
        // a permanent divider, and it must not sit under every channel you
        // have already read.
        let unread_since = st
            .memberships
            .get(&channel_id)
            .filter(|_| st.unread(&channel_id).is_unread())
            .and_then(|m| m.last_viewed_at)
            .filter(|at| *at > 0);

        let empty = crate::state::ChannelFeed::default();
        let feed = st.feeds.get(&channel_id).unwrap_or(&empty);
        let at_latest = feed.at_latest || feed.posts.is_empty();
        self.behind.set(!at_latest);
        let title = st.channel_title(&channel);
        let items = build_feed_items(
            &feed.posts,
            &st,
            st.crt_enabled,
            unread_since,
            feed.at_oldest.then_some(title.as_str()),
        );
        drop(st);

        // Whether this is a redraw of what is already on screen, as opposed
        // to arriving in a different channel — the scroll position is only
        // worth keeping in the first case.
        let same_channel = self.showing.borrow().as_deref() == Some(channel_id.as_str());
        if scroll_trace_enabled() {
            tracing::info!(
                target: "matterfast::scroll",
                event = "feed-refresh",
                channel_id,
                same_channel,
                at_latest,
                following = self.list.is_following_tail(),
                rows = items.len(),
                "scroll trace"
            );
        }

        if same_channel {
            let old: Vec<String> = self.items.borrow().iter().map(FeedItem::key).collect();
            let new: Vec<String> = items.iter().map(FeedItem::key).collect();
            *self.items.borrow_mut() = items;
            if let Some((removed, added)) = splice_plan(&old, &new) {
                self.list.splice(removed, added);
            }
        } else {
            *self.showing.borrow_mut() = Some(channel_id);
            self.pagination_armed.set(true);
            self.newer_armed.set(true);
            self.member_count.set(None);
            self.highlight.borrow_mut().take();
            let unread_at = items
                .iter()
                .position(|item| matches!(item, FeedItem::Unread));
            self.list.reset(items.len());
            *self.items.borrow_mut() = items;
            match unread_at {
                // A block of history that does not reach the newest message
                // opens where reading stopped, which is what it was fetched
                // around; scrolling to its end would land in the past.
                Some(index) if !at_latest => self.scroll_near(index),
                _ => self.scroll_to_newest("channel-open"),
            }
        }
        crate::ui::refresh(cx);
    }
}
