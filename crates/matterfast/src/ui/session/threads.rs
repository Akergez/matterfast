//! The right panel's thread view and the inbox.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::UserThread;

use super::hydrate::hydrate_authors;
use super::ui::Ui;
use crate::runtime;
use crate::state::ChannelFeed;
use crate::timefmt::now_ms;
use crate::ui::constants::{FOCUS_ON_ARRIVAL, INBOX_PAGE};
use crate::ui::rhs::PanelMode;

/// The followed threads to hold after the newest page of them was asked for
/// again: that page, and after it whatever older pages had been fetched
/// before. Asking again must not throw away what somebody scrolled down to.
///
/// A thread that is in both is the fresh one's. A held thread that is not on
/// the page and is newer than the page's last was unfollowed, or the page
/// would have had it; it goes.
fn newest_with_older(page: Vec<UserThread>, held: Vec<UserThread>) -> Vec<UserThread> {
    let when = |thread: &UserThread| thread.last_reply_at.max(thread.post.create_at);
    let Some(oldest) = page.last().map(when) else {
        return page;
    };
    let fresh: HashSet<String> = page.iter().map(|thread| thread.id.clone()).collect();
    let mut threads = page;
    threads.extend(
        held.into_iter()
            .filter(|thread| !fresh.contains(&thread.id) && when(thread) <= oldest),
    );
    threads
}

impl Ui {
    /// Asks for the followed threads after the ones held: the server keeps
    /// the list newest first and hands it over a page at a time.
    pub(crate) fn load_older_threads(self: &Rc<Self>, cx: &mut App) {
        let (client, team_id, before) = {
            let st = self.state.borrow();
            let (Some(team), Some(last)) = (st.current_team.clone(), st.thread_inbox.last())
            else {
                return;
            };
            (st.client.clone(), team, last.id.clone())
        };
        let ui = self.clone();
        runtime::spawn(
            async move {
                let threads = client
                    .my_threads_before(&team_id, &before, INBOX_PAGE)
                    .await?;
                let ids: Vec<String> = threads
                    .threads
                    .iter()
                    .map(|thread| thread.post.user_id.clone())
                    .filter(|id| !id.is_empty())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                let authors = match ids.is_empty() {
                    true => Vec::new(),
                    false => client.users_by_ids(&ids).await.unwrap_or_default(),
                };
                Ok::<_, mattermost_api::Error>((threads, authors))
            },
            move |result, cx| {
                let Ok((threads, authors)) = result else {
                    return;
                };
                {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.thread_inbox_total = threads.total;
                    let held: HashSet<String> =
                        st.thread_inbox.iter().map(|thread| thread.id.clone()).collect();
                    let older: Vec<UserThread> = threads
                        .threads
                        .into_iter()
                        .filter(|thread| !held.contains(&thread.id))
                        .collect();
                    // A page with nothing new in it is the end, whatever the
                    // count says: the button must not be there to press again.
                    if older.is_empty() {
                        st.thread_inbox_total = st.thread_inbox.len() as i64;
                    }
                    st.thread_inbox.extend(older);
                }
                ui.refresh_messages(cx);
            },
        );
        let _ = cx;
    }

    pub(crate) fn open_thread(self: &Rc<Self>, root_id: String, cx: &mut App) {
        // Flush the previous thread's reply before the box is reused.
        self.save_thread_draft(cx);
        self.right.set_mode(PanelMode::Thread(root_id.clone()), cx);
        self.refresh_panel_mode(cx);
        self.overlay.set_show_sidebar(true, cx);
        self.restore_thread_draft(cx);
        let following = {
            let st = self.state.borrow();
            st.thread_inbox
                .iter()
                .any(|t| t.id == root_id && t.is_following)
        };
        self.right.set_following(following, cx);

        // Opening a thread is reading it, so its unread count should go —
        // the inbox badge only ever grew before.
        self.mark_thread_read(&root_id);

        // Seed the panel from the root we already hold, so it draws the
        // message immediately and fills in the replies when they arrive.
        // Waiting for the round trip is what made opening a thread feel slow
        // and, on a slow link, look like nothing had happened.
        {
            let mut st = self.state.borrow_mut();
            if !st.threads.contains_key(&root_id) {
                if let Some(root) = st.post(&root_id) {
                    st.threads
                        .insert(root_id.clone(), ChannelFeed::from_posts(vec![root]));
                }
            }
        }
        self.refresh_thread_panel(cx);

        // Always refetch. A thread we opened earlier may have grown, and the
        // root's reply count is not enough to tell which replies we hold.
        let (client, crt) = {
            let st = self.state.borrow();
            (st.client.clone(), st.crt_enabled)
        };
        let id = root_id.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                let list = client.post_thread(&id, crt).await?;
                let (authors, statuses) = hydrate_authors(&client, &list).await;
                Ok::<_, mattermost_api::Error>((list, authors, statuses))
            },
            move |result, cx| match result {
                Ok((list, authors, statuses)) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        st.threads
                            .insert(root_id.clone(), ChannelFeed::from_list(&list));
                    }
                    // Both panes here: a fetched thread can change the reply
                    // footer in the feed behind it. Off the click's critical
                    // path, so the cost does not show.
                    ui.refresh_messages(cx);
                    // A slow fetch can lose the race to a click elsewhere —
                    // only steal focus into the reply box if the panel is
                    // still showing the thread this answer is for.
                    if ui.right.mode(cx) == PanelMode::Thread(root_id) && FOCUS_ON_ARRIVAL {
                        ui.right.focus_composer(cx);
                    }
                }
                Err(e) => {
                    ui.toast(&format!("Could not load the thread: {e}"), cx);
                    // Stop the panel claiming it is still loading. Whatever we
                    // hold of the root is better than a spinner that never
                    // resolves.
                    let root = ui.state.borrow().find_post(&root_id).cloned();
                    if let Some(root) = root {
                        ui.state.borrow_mut().threads.insert(
                            root_id,
                            ChannelFeed {
                                posts: vec![root],
                                ..Default::default()
                            },
                        );
                    }
                    ui.refresh_messages(cx);
                }
            },
        );
    }

    /// Tells the server a thread with unread replies has been read.
    fn mark_thread_read(self: &Rc<Self>, root_id: &str) {
        let (client, team_id, unread) = {
            let st = self.state.borrow();
            let unread = st
                .thread_inbox
                .iter()
                .any(|t| t.id == root_id && t.unread_replies > 0);
            (
                st.client.clone(),
                st.current_team.clone().unwrap_or_default(),
                unread,
            )
        };
        if !unread || team_id.is_empty() {
            return;
        }
        let id = root_id.to_string();
        let ui = self.clone();
        let now = now_ms();
        runtime::spawn(
            async move { client.mark_thread_read(&team_id, &id, now).await },
            move |result, cx| {
                if result.is_ok() {
                    ui.load_inbox(cx);
                }
            },
        );
    }

    /// Puts the inbox in front of the conversations, or — asked for a second
    /// time — takes it away again: the button in the title bar and the
    /// shortcut go both ways, as they did when the inbox was a panel.
    pub(crate) fn open_inbox(self: &Rc<Self>, cx: &mut App) {
        if self.channels.showing_inbox() {
            self.channels.show_inbox(false, cx);
            return;
        }
        self.channels.show_inbox(true, cx);
        // On a phone the list it is a tab of is a page of its own, and has
        // to be the one in front.
        self.split.set_show_content(false, cx);

        self.refresh_messages(cx);
        self.load_inbox(cx);
    }

    /// Fetches recent mentions and the thread inbox for the current team.
    pub(crate) fn load_inbox(self: &Rc<Self>, _cx: &mut App) {
        let (client, team_id, username, crt, me) = {
            let st = self.state.borrow();
            let Some(team) = st.current_team.clone() else {
                return;
            };
            (
                st.client.clone(),
                team,
                st.me.username.clone(),
                st.crt_enabled,
                st.me.id.clone(),
            )
        };

        let ui = self.clone();
        runtime::spawn(
            async move {
                // Mattermost's "Recent Mentions" is literally a search for your
                // mention keys; @username is the one every account has.
                let mentions = client
                    .search_posts(
                        &team_id,
                        &mattermost_api::rest::PostSearch::new(&format!("@{username}")),
                    )
                    .await
                    .ok();
                let threads = if crt {
                    client.my_threads(&team_id, false, INBOX_PAGE).await.ok()
                } else {
                    None
                };

                // Saved posts are a preference list of ids; the posts
                // themselves have to be fetched, or the saved tab can only
                // show whatever a channel happened to load.
                let saved = client.flagged_posts(&me, INBOX_PAGE).await.ok();

                let mut ids: HashSet<String> = HashSet::new();
                if let Some(s) = &saved {
                    ids.extend(s.posts.values().map(|p| p.user_id.clone()));
                }
                if let Some(m) = &mentions {
                    ids.extend(m.posts.posts.values().map(|p| p.user_id.clone()));
                }
                if let Some(t) = &threads {
                    ids.extend(t.threads.iter().map(|t| t.post.user_id.clone()));
                }
                let ids: Vec<String> = ids.into_iter().filter(|i| !i.is_empty()).collect();
                let authors = if ids.is_empty() {
                    Vec::new()
                } else {
                    client.users_by_ids(&ids).await.unwrap_or_default()
                };
                (mentions, threads, saved, authors)
            },
            move |(mentions, threads, saved, authors), cx| {
                {
                    let mut st = ui.state.borrow_mut();
                    for user in authors {
                        st.users.insert(user.id.clone(), user);
                    }
                    if let Some(results) = mentions {
                        st.mentions = results.posts.ordered().cloned().collect();
                    }
                    if let Some(threads) = threads {
                        st.thread_inbox_total = threads.total;
                        let held = std::mem::take(&mut st.thread_inbox);
                        st.thread_inbox = newest_with_older(threads.threads, held);
                    }
                    // Filed under their own channels, so the saved tab and the
                    // conversation agree about what a post says.
                    if let Some(saved) = saved {
                        for post in saved.posts.values() {
                            st.apply_post(post.clone());
                        }
                    }
                }
                ui.refresh_messages(cx);
            },
        );
    }
}
