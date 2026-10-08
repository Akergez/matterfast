//! Loading messages newer and older than the ones held.

use std::rc::Rc;

use gpui_kit::App;

use super::hydrate::hydrate_authors;
use super::ui::Ui;
use crate::runtime;
use crate::state::ChannelFeed;
use crate::ui::chat;
use crate::ui::constants::INITIAL_POSTS;

impl Ui {
    /// Fetches what came after the newest message we hold, a page at a time
    /// until the server says there is nothing later. Used when the socket has
    /// been away long enough that the feed has a hole in it — scrolling up
    /// finds older messages, and nothing else would find the ones in the
    /// middle.
    pub(crate) fn load_newer(self: &Rc<Self>, channel_id: String, _cx: &mut App) {
        let (client, crt, newest) = {
            let st = self.state.borrow();
            let Some(feed) = st.feeds.get(&channel_id) else {
                return;
            };
            let Some(newest) = feed.posts.last().map(|p| p.id.clone()) else {
                return;
            };
            (st.client.clone(), st.crt_enabled, newest)
        };

        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client
                    .posts_after(&channel_id, &newest, INITIAL_POSTS, crt)
                    .await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((channel_id, posts, authors, statuses))
            },
            move |result, cx| {
                let Ok((channel_id, posts, authors, statuses)) = result else {
                    return;
                };
                let caught_up = {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.apply_statuses(statuses);
                    let newer = ChannelFeed::from_list(&posts);
                    // An empty page is the end whatever else it claims;
                    // asking again would ask the same question forever.
                    let caught_up = newer.at_latest || newer.posts.is_empty();
                    if let Some(feed) = st.feeds.get_mut(&channel_id) {
                        for post in newer.posts {
                            feed.upsert(post);
                        }
                        feed.at_latest = caught_up;
                    }
                    caught_up
                };
                ui.refresh_messages(cx);
                ui.store_posts(posts.chronological().into_iter().cloned().collect(), cx);
                if !caught_up {
                    ui.load_newer(channel_id, cx);
                }
            },
        );
    }

    /// Fetches the one page after the newest message held of the channel on
    /// screen, when what is held stops short of the present: what reaching
    /// the bottom of such a block asks for, as reaching the top asks for the
    /// page before. One page for one arrival at the bottom — a block from
    /// months back is not walked to today because somebody read to its end.
    pub(crate) fn load_newer_page(self: &Rc<Self>, _cx: &mut App) {
        if self.chat.loading_newer.get() {
            return;
        }
        let (client, crt, channel_id, newest) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let Some(feed) = st.feeds.get(&channel_id).filter(|feed| !feed.at_latest) else {
                return;
            };
            let Some(newest) = feed.posts.last().map(|p| p.id.clone()) else {
                return;
            };
            (st.client.clone(), st.crt_enabled, channel_id, newest)
        };

        self.chat.loading_newer.set(true);
        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client
                    .posts_after(&channel_id, &newest, INITIAL_POSTS, crt)
                    .await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((channel_id, posts, authors, statuses))
            },
            move |result, cx| {
                ui.chat.loading_newer.set(false);
                let Ok((channel_id, posts, authors, statuses)) = result else {
                    // Asked for again on the next arrival at the bottom.
                    ui.chat.newer_armed.set(true);
                    return;
                };
                {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.apply_statuses(statuses);
                    let newer = ChannelFeed::from_list(&posts);
                    let caught_up = newer.at_latest || newer.posts.is_empty();
                    if let Some(feed) = st.feeds.get_mut(&channel_id) {
                        for post in newer.posts {
                            feed.upsert(post);
                        }
                        feed.at_latest = caught_up;
                    }
                }
                // A list that had been read to its end follows the end, and
                // the new end is a page further on than anything read. The
                // page goes on from where reading was instead: the last rows
                // that were there at the top, and the new ones under them.
                let at_end = ui.chat.list.is_following_tail();
                let rows = ui.chat.items.borrow().len();
                ui.refresh_messages(cx);
                if at_end {
                    ui.chat.scroll_near(rows);
                }
                ui.store_posts(posts.chronological().into_iter().cloned().collect(), cx);
            },
        );
    }

    /// Fetches the newest page of a channel whose block stops short of the
    /// present, and then does `then`. The page joins what is held if they
    /// meet and takes its place if they do not (`ChannelFeed::land`), so it
    /// is one request however far behind the block was — walking forward
    /// from a block of last month a page at a time was as many requests as
    /// it took.
    pub(crate) fn load_latest(
        self: &Rc<Self>,
        channel_id: String,
        then: impl FnOnce(&Rc<Ui>, &mut App) + 'static,
    ) {
        let (client, crt) = {
            let st = self.state.borrow();
            (st.client.clone(), st.crt_enabled)
        };
        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client
                    .posts_for_channel(&channel_id, 0, INITIAL_POSTS, crt)
                    .await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((channel_id, posts, authors, statuses))
            },
            move |result, cx| {
                if let Ok((channel_id, posts, authors, statuses)) = result {
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        let page = ChannelFeed::from_list(&posts);
                        let feed = st.feeds.entry(channel_id).or_default();
                        if page.posts.is_empty() {
                            // Nothing has been said there at all.
                            feed.at_latest = true;
                        }
                        feed.land(page.posts, page.at_oldest, true);
                    }
                    ui.refresh_messages(cx);
                    ui.store_posts(posts.chronological().into_iter().cloned().collect(), cx);
                }
                then(&ui, cx);
            },
        );
    }

    /// Leaves a block of history for the newest messages: what the button
    /// over the feed does when the block does not run to them.
    pub(crate) fn return_to_present(self: &Rc<Self>, _cx: &mut App) {
        let Some(channel_id) = self.state.borrow().current_channel.clone() else {
            return;
        };
        self.load_latest(channel_id, |ui, cx| {
            ui.chat.scroll_to_newest("return-to-present");
            crate::ui::refresh(cx);
        });
    }

    /// Fetches the page of messages before the oldest one we hold.
    ///
    /// Only one at a time, and never past the beginning: reaching the top of a
    /// short channel would otherwise ask for the same empty page on every
    /// scroll event.
    pub(crate) fn load_older(self: &Rc<Self>, cx: &mut App) {
        if self.loading_older.get() {
            if chat::scroll_trace_enabled() {
                tracing::info!(
                    target: "matterfast::scroll",
                    event = "pagination-suppressed",
                    reason = "already-loading",
                    "scroll trace"
                );
            }
            return;
        }
        let (client, crt, channel_id, oldest) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let Some(feed) = st.feeds.get(&channel_id) else {
                return;
            };
            if feed.at_oldest {
                if chat::scroll_trace_enabled() {
                    tracing::info!(
                        target: "matterfast::scroll",
                        event = "pagination-suppressed",
                        reason = "at-oldest",
                        channel_id,
                        "scroll trace"
                    );
                }
                return;
            }
            let Some(oldest) = feed.posts.first().map(|p| p.id.clone()) else {
                return;
            };
            (st.client.clone(), st.crt_enabled, channel_id, oldest)
        };

        self.loading_older.set(true);
        if chat::scroll_trace_enabled() {
            tracing::info!(
                target: "matterfast::scroll",
                event = "pagination-request",
                channel_id,
                oldest,
                "scroll trace"
            );
        }
        self.chat.set_loading_older(true, cx);
        let ui = self.clone();
        runtime::spawn(
            async move {
                let posts = client
                    .posts_before(&channel_id, &oldest, INITIAL_POSTS, crt)
                    .await?;
                let (authors, statuses) = hydrate_authors(&client, &posts).await;
                Ok::<_, mattermost_api::Error>((channel_id, posts, authors, statuses))
            },
            move |result, cx| {
                ui.chat.set_loading_older(false, cx);
                let Ok((channel_id, posts, authors, statuses)) = result else {
                    if chat::scroll_trace_enabled() {
                        tracing::warn!(
                            target: "matterfast::scroll",
                            event = "pagination-error",
                            "scroll trace"
                        );
                    }
                    ui.loading_older.set(false);
                    ui.chat.retry_older_on_next_edge_change(cx);
                    return;
                };
                if chat::scroll_trace_enabled() {
                    tracing::info!(
                        target: "matterfast::scroll",
                        event = "pagination-response",
                        channel_id,
                        posts = posts.posts.len(),
                        "scroll trace"
                    );
                }
                let kept;
                // Whether the channel this page belongs to is still the one
                // on screen; a page for a feed nobody is looking at only goes
                // into the state.
                let showing;
                {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.apply_statuses(statuses);
                    let older = ChannelFeed::from_list(&posts);
                    // An empty page means there is nothing before this, and
                    // the feed should stop asking.
                    let exhausted = older.posts.is_empty();
                    kept = older.posts.clone();
                    let now_at_oldest = exhausted || older.at_oldest;
                    showing = st.current_channel.as_deref() == Some(channel_id.as_str());
                    if let Some(feed) = st.feeds.get_mut(&channel_id) {
                        for post in older.posts {
                            feed.upsert(post);
                        }
                        feed.at_oldest = now_at_oldest;
                    }
                }
                // The new rows are a splice at the top of the list: nothing
                // the reader is looking at is rebuilt, and the list keeps its
                // own anchor across it. This used to redraw everything ever
                // paged into the channel on every page turn.
                if showing {
                    ui.chat.refresh(&ui.state, cx);
                }
                ui.loading_older.set(false);
                ui.store_posts(kept, cx);
            },
        );
    }
}
