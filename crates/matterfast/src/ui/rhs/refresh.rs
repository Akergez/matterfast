use std::rc::Rc;

use gpui_kit::{App, FollowMode};
use mattermost_api::models::Post;

use super::build_inbox::build_inbox;
use super::build_thread_rows::build_thread_rows;
use super::panel_mode::PanelMode;
use super::right_panel::RightPanel;
use super::search_row::SearchRow;
use super::thread_row::ThreadRow;
use crate::state::{AppState, SharedState};
use crate::ui::message;

impl RightPanel {
    /// Rebuilds whatever the panel is currently showing from the state.
    pub fn refresh(&self, state: &SharedState, cx: &mut App) {
        let st = state.borrow();
        match self.mode(cx) {
            PanelMode::Hidden => {}
            PanelMode::Thread(root_id) => self.refresh_thread(&root_id, &st),
            PanelMode::Inbox => *self.inbox.borrow_mut() = build_inbox(&st),
            PanelMode::Search(_) => {
                self.searching.set(st.searching);
                self.search_more.set(st.search_more);
                *self.search.borrow_mut() = st
                    .search_results
                    .iter()
                    .map(|post| SearchRow {
                        // Which channel a hit came from is most of what makes
                        // it useful.
                        channel: st
                            .channel(&post.channel_id)
                            .map(|c| st.channel_title(c))
                            .unwrap_or_default(),
                        post: Rc::new(post.clone()),
                        body: message::message_markdown(&post.message, &st).into(),
                    })
                    .collect();
            }
        }
        drop(st);
        cx.refresh_windows();
    }

    fn refresh_thread(&self, root_id: &str, st: &AppState) {
        // While the replies are in flight, show the root on its own if we
        // already hold it — the reader clicked a message they were looking at,
        // and an empty panel makes the click feel lost.
        let posts: Option<Vec<Post>> = match st.threads.get(root_id) {
            Some(feed) => Some(feed.posts.clone()),
            None => st.find_post(root_id).map(|root| vec![root.clone()]),
        };
        let Some(posts) = posts else {
            self.thread_loaded.set(false);
            return;
        };
        self.thread_loaded.set(true);
        *self.thread_channel.borrow_mut() = posts
            .first()
            .and_then(|p| st.channel(&p.channel_id))
            .map(|c| st.channel_title(c))
            .unwrap_or_default();

        let rows = build_thread_rows(posts, root_id, st);
        let old: Vec<String> = self.thread.borrow().iter().map(ThreadRow::key).collect();
        let new: Vec<String> = rows.iter().map(ThreadRow::key).collect();
        *self.thread.borrow_mut() = rows;
        if old.is_empty() {
            self.list.reset(new.len());
            self.list.set_follow_mode(FollowMode::Tail);
        } else if old != new {
            // A thread is a screenful, not a channel's history: the common
            // case is one reply added at the end.
            let common = old
                .iter()
                .zip(new.iter())
                .take_while(|(a, b)| a == b)
                .count();
            self.list.splice(common..old.len(), new.len() - common);
        }
    }
}
