//! The snapshot the next launch opens with.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::Post;

use super::ui::Ui;
use crate::runtime;

/// Once every ten minutes of running is plenty for a cache that grows a
/// screenful at a time.
const PRUNE_EVERY: std::time::Duration = std::time::Duration::from_secs(600);

impl Ui {
    /// Whether the store is due a trim, noting that it is being given one.
    pub(crate) fn should_prune(&self) -> bool {
        let prune = self
            .pruned_at
            .get()
            .is_none_or(|at| at.elapsed() >= PRUNE_EVERY);
        if prune {
            self.pruned_at.set(Some(std::time::Instant::now()));
        }
        prune
    }

    /// Writes the snapshot the next launch will open with.
    ///
    /// Debounced hard: this serialises a chunk of state, and the only thing
    /// that matters is that it ran reasonably recently before the app closed.
    pub(crate) fn schedule_snapshot(self: &Rc<Self>, _cx: &mut App) {
        if self.snapshot_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        runtime::after(std::time::Duration::from_secs(5), move |cx| {
            ui.snapshot_pending.set(false);
            ui.save_snapshot(cx);
        });
    }

    pub(crate) fn capture_scroll_anchor(&self, cx: &mut App) {
        let Some((channel_id, post_id)) = self.chat.current_anchor(cx) else {
            return;
        };
        let mut st = self.state.borrow_mut();
        match post_id {
            Some(post_id) => {
                st.scroll_anchors.insert(channel_id, post_id);
            }
            None => {
                st.scroll_anchors.remove(&channel_id);
            }
        }
    }

    pub(crate) fn save_snapshot(&self, cx: &mut App) {
        self.capture_scroll_anchor(cx);
        let st = self.state.borrow();
        crate::cache::save(&crate::cache::Snapshot {
            server: st.client.site_url().to_string(),
            current_team: st.current_team.clone(),
            current_channel: st.current_channel.clone(),
            scroll_anchors: st.scroll_anchors.clone(),
        });
        drop(st);

        // The content goes to the store, which keeps every channel rather than
        // the handful a single file could hold.
        let Some(store) = self.store.borrow().clone() else {
            return;
        };
        let prune = self.should_prune();
        let (channels, members, users, posts) = {
            let st = self.state.borrow();
            let posts: Vec<Post> = st
                .feeds
                .values()
                .flat_map(|feed| feed.posts.iter().cloned())
                .collect();
            (
                st.channels.values().cloned().collect::<Vec<_>>(),
                st.memberships.values().cloned().collect::<Vec<_>>(),
                st.users.values().cloned().collect::<Vec<_>>(),
                posts,
            )
        };
        runtime::spawn(
            async move {
                // Failures here cost a slower next launch and nothing else, so
                // they are logged rather than surfaced.
                if let Err(e) = store.save_channels(channels, members).await {
                    tracing::warn!(error = %e, "could not store the channel list");
                }
                if let Err(e) = store.save_users(users).await {
                    tracing::warn!(error = %e, "could not store the users");
                }
                if let Err(e) = store.save_posts(posts).await {
                    tracing::warn!(error = %e, "could not store the messages");
                }
                // Occasionally, not on every write: switching channels
                // writes a snapshot each time, and scanning every post to
                // delete nothing is pure work.
                if prune {
                    if let Err(e) = store.prune(crate::store::DEFAULT_KEEP_PER_CHANNEL).await {
                        tracing::warn!(error = %e, "could not trim the store");
                    }
                }
            },
            |_, _| {},
        );
    }
}
