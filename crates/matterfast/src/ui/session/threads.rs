//! The right panel's thread view and the inbox.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::App;

use super::hydrate::hydrate_authors;
use super::ui::Ui;
use crate::runtime;
use crate::state::ChannelFeed;
use crate::timefmt::now_ms;
use crate::ui::constants::{FOCUS_ON_ARRIVAL, INBOX_PAGE};
use crate::ui::rhs::PanelMode;

impl Ui {
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

    pub(crate) fn open_inbox(self: &Rc<Self>, cx: &mut App) {
        self.right.set_mode(PanelMode::Inbox, cx);
        self.refresh_panel_mode(cx);
        self.overlay.set_show_sidebar(true, cx);

        // Land on whichever tab has something to show: unread threads with no
        // mentions would otherwise open onto an empty list.
        let st = self.state.borrow();
        let threads_first = st.mentions.is_empty() && st.unread_threads() > 0;
        drop(st);
        if threads_first {
            self.right.show_threads_tab(cx);
        }

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
                        st.thread_inbox = threads.threads;
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
