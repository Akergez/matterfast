use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    /// Finds a post anywhere we are holding one — channel feeds first, then
    /// threads, since a reply only lives in the latter under CRT.
    pub fn post(&self, post_id: &str) -> Option<Post> {
        self.find_post(post_id).cloned()
    }

    /// Finds a post in any feed we hold.
    pub fn find_post(&self, post_id: &str) -> Option<&Post> {
        self.feeds
            .values()
            .chain(self.threads.values())
            .find_map(|feed| feed.posts.iter().find(|p| p.id == post_id))
    }

    /// Applies a post to whichever feeds hold it: the channel, the thread, or
    /// both.
    ///
    /// Under collapsed reply threads a reply must **not** enter the channel
    /// feed — that is the whole point of the mode, and the webapp's reducers
    /// return early for exactly this case.
    pub fn apply_post(&mut self, post: Post) {
        let root = post.thread_root().to_string();

        if let Some(thread) = self.threads.get_mut(&root) {
            thread.upsert(post.clone());
        }

        if post.is_reply() {
            // Keep the root's reply counter live, since that is the only
            // affordance leading into the thread.
            let new = self.replies.is_new(&post);
            if let Some(feed) = self.feeds.get_mut(&post.channel_id) {
                if let Some(root_post) = feed.posts.iter_mut().find(|p| p.id == root) {
                    if new {
                        root_post.reply_count += 1;
                    }
                    root_post.last_reply_at = root_post.last_reply_at.max(post.create_at);
                }
            }
            if self.crt_enabled {
                return;
            }
        }

        // Into the block only where the block runs to the present: see
        // `ChannelFeed::live`.
        if let Some(feed) = self.feeds.get_mut(&post.channel_id) {
            feed.live(post);
        }
    }

    /// Drops a post from every feed and thread that holds it.
    ///
    /// Used to retire an optimistic copy: relying on `pending_post_id` coming
    /// back on the echo is not enough, because the REST response and the
    /// websocket event race, and only one of them is guaranteed to carry it.
    pub fn remove_post(&mut self, post_id: &str) {
        if self.replies.retire(post_id) {
            // It was a reply of ours that never made it: find its root
            // through the thread that holds it and take the count back.
            let root = self.threads.iter().find_map(|(root, thread)| {
                thread
                    .posts
                    .iter()
                    .any(|p| p.id == post_id)
                    .then(|| root.clone())
            });
            if let Some(root) = root {
                for feed in self.feeds.values_mut() {
                    if let Some(root_post) = feed.posts.iter_mut().find(|p| p.id == root) {
                        root_post.reply_count = (root_post.reply_count - 1).max(0);
                    }
                }
            }
        }
        for feed in self.feeds.values_mut().chain(self.threads.values_mut()) {
            feed.remove(post_id);
        }
    }
}
