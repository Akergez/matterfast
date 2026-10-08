//! Fetching ahead of the person, and what is kept of it on disk.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::Post;

use super::hydrate::hydrate_authors;
use super::ui::Ui;
use crate::runtime;
use crate::state::ChannelFeed;
use crate::ui::constants::INITIAL_POSTS;
use crate::ui::mentions::mentioned_names;

impl Ui {
    /// Looks up people named in messages that we do not hold.
    ///
    /// A mention is only highlighted once the name resolves, and a channel you
    /// have just opened is full of names you may never have seen — without
    /// this, every one of them reads as plain text until they happen to post.
    pub(crate) fn resolve_mentions(self: &Rc<Self>, _cx: &mut App) {
        let (client, wanted) = {
            let st = self.state.borrow();
            let known: HashSet<&str> = st.users.values().map(|u| u.username.as_str()).collect();
            let mut wanted: HashSet<String> = HashSet::new();
            let feed = st.current_channel.as_ref().and_then(|id| st.feeds.get(id));
            for post in feed.into_iter().flat_map(|feed| feed.posts.iter()) {
                for name in mentioned_names(&post.message) {
                    if !known.contains(name.as_str()) {
                        wanted.insert(name);
                    }
                }
            }
            (st.client.clone(), wanted)
        };
        if wanted.is_empty() {
            return;
        }

        let names: Vec<String> = wanted.into_iter().take(50).collect();
        let ui = self.clone();
        runtime::spawn(
            async move { client.users_by_usernames(&names).await },
            move |result, cx| {
                let Ok(users) = result else { return };
                if users.is_empty() {
                    return;
                }
                {
                    let mut st = ui.state.borrow_mut();
                    for user in users {
                        st.users.insert(user.id.clone(), user);
                    }
                }
                ui.refresh_messages(cx);
            },
        );
    }

    /// Quietly fetches the channels most likely to be opened next.
    ///
    /// Switching to a channel that has never been read waits on the network;
    /// the ones with something unread are exactly the ones about to be
    /// clicked, so their first page is fetched before it is asked for. Three
    /// of them, because this is a guess and a wrong guess should be cheap.
    pub(crate) fn preload_unread(self: &Rc<Self>, _cx: &mut App) {
        const PRELOAD: usize = 3;

        let (client, crt, wanted) = {
            let st = self.state.borrow();
            let wanted: Vec<String> = st
                .chat_list(None)
                .into_iter()
                .map(|c| c.id.clone())
                .filter(|id| st.unread(id).is_unread() && !st.feeds.contains_key(id))
                .take(PRELOAD)
                .collect();
            (st.client.clone(), st.crt_enabled, wanted)
        };

        for channel_id in wanted {
            let client = client.clone();
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
                    let Ok((channel_id, posts, authors, statuses)) = result else {
                        return;
                    };
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        // Only if it is still absent: the person may have
                        // opened it while this was in flight, and that copy is
                        // the one being read.
                        st.feeds
                            .entry(channel_id)
                            .or_insert_with(|| ChannelFeed::from_list(&posts));
                    }
                    ui.store_posts(posts.chronological().into_iter().cloned().collect(), cx);
                },
            );
        }
    }

    /// Files posts in the local store. Fire and forget: a failure costs a
    /// slower next launch and nothing on this one.
    pub(crate) fn store_posts(&self, posts: Vec<Post>, _cx: &mut App) {
        if posts.is_empty() {
            return;
        }
        let Some(store) = self.store.borrow().clone() else {
            return;
        };
        // Counts as a prune pass for the snapshot's schedule, as it always has.
        self.should_prune();
        runtime::spawn(
            async move {
                if let Err(e) = store.save_posts(posts).await {
                    tracing::warn!(error = %e, "could not store those messages");
                }
            },
            |_, _| {},
        );
    }
}
