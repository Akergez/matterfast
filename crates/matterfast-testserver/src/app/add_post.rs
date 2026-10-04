use serde_json::{json, Value};

use super::post::Post;
use super::state::App;
use crate::clock::now;
use crate::ids::id;

impl App {
    pub(crate) fn add_post_with_pending(
        &self,
        channel_id: &str,
        user_id: &str,
        message: &str,
        root_id: &str,
        pending_post_id: &str,
    ) -> Value {
        let mut post = self.add_post(channel_id, user_id, message, root_id);
        // The real server echoes this back so clients can retire their
        // optimistic copy; not doing so makes every sent message appear twice.
        post["pending_post_id"] = json!(pending_post_id);
        let post_id = post["id"].clone();
        let mut db = self.db.lock().unwrap();
        if let Some(stored) = db.posts.iter_mut().find(|p| p.v["id"] == post_id) {
            stored.v["pending_post_id"] = json!(pending_post_id);
        }
        post
    }

    pub(crate) fn add_post(
        &self,
        channel_id: &str,
        user_id: &str,
        message: &str,
        root_id: &str,
    ) -> Value {
        let mut db = self.db.lock().unwrap();
        db.next_id += 1;
        let post_id = id("p", db.next_id);
        // Seeding happens inside one millisecond, and a real server would not
        // hand out identical timestamps for a whole conversation — nudge each
        // post forward so ordering is deterministic.
        let at = now() + db.next_id;
        let post = json!({
            "id": post_id,
            "create_at": at,
            "update_at": at,
            "edit_at": 0,
            "delete_at": 0,
            "is_pinned": false,
            "user_id": user_id,
            "channel_id": channel_id,
            "root_id": root_id,
            "original_id": "",
            "message": message,
            "type": "",
            "props": {},
            "hashtags": "",
            "file_ids": [],
            "pending_post_id": "",
            "reply_count": 0,
            "last_reply_at": 0,
            "participants": null,
            "metadata": {},
        });
        db.posts.push(Post { v: post.clone() });

        // Keep the root's reply_count honest — the client renders it.
        if !root_id.is_empty() {
            let replies = db
                .posts
                .iter()
                .filter(|p| p.v["root_id"] == root_id)
                .count() as i64;
            if let Some(root) = db.posts.iter_mut().find(|p| p.v["id"] == root_id) {
                root.v["reply_count"] = json!(replies);
                root.v["last_reply_at"] = json!(at);
            }
        }
        post
    }
}
