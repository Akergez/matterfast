use std::collections::HashSet;

use mattermost_api::models::Post;

/// Which replies have already been added to their root's reply counter.
///
/// One reply reaches us up to three times — our own optimistic copy, the
/// answer to the request that sent it, and the websocket's `posted` — under
/// two different ids. Counting each arrival made a single reply read
/// "3 replies".
#[derive(Default)]
pub struct ReplyLedger {
    /// Every id a counted reply has been seen under, pending ids included.
    counted: HashSet<String>,
    /// Pending ids the server has since answered for.
    confirmed: HashSet<String>,
}

impl ReplyLedger {
    /// Whether this arrival is a reply nobody has counted yet.
    pub fn is_new(&mut self, post: &Post) -> bool {
        let pending = post.pending_post_id.as_str();
        let seen = self.counted.contains(&post.id)
            || (!pending.is_empty() && self.counted.contains(pending));
        self.counted.insert(post.id.clone());
        if !pending.is_empty() {
            self.counted.insert(pending.to_string());
            if !post.is_pending() {
                self.confirmed.insert(pending.to_string());
            }
        }
        // An edit is news about a reply, not a new one — and it may be about
        // one from before we were looking.
        !seen && post.edit_at == 0
    }

    /// An optimistic copy is being taken down. True when it had been counted
    /// and nothing real has taken its place, so the count should go back.
    pub fn retire(&mut self, post_id: &str) -> bool {
        if self.confirmed.contains(post_id) {
            return false;
        }
        self.counted.remove(post_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(id: &str, pending: &str) -> Post {
        Post {
            id: id.into(),
            create_at: 200,
            root_id: "root".into(),
            pending_post_id: pending.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_reply_of_ours_is_counted_once_however_it_arrives() {
        // Optimistic copy, then the request's answer, then the websocket.
        let mut ledger = ReplyLedger::default();
        assert!(ledger.is_new(&reply("pending-1", "pending-1")));
        assert!(ledger.retire("pending-1"), "taken down before the answer");
        assert!(ledger.is_new(&reply("real", "pending-1")));
        assert!(!ledger.is_new(&reply("real", "")));

        // The websocket wins the race: the answer must not undo the count.
        let mut ledger = ReplyLedger::default();
        assert!(ledger.is_new(&reply("pending-1", "pending-1")));
        assert!(!ledger.is_new(&reply("real", "pending-1")));
        assert!(!ledger.retire("pending-1"));
        assert!(!ledger.is_new(&reply("real", "pending-1")));
    }

    #[test]
    fn a_reply_that_failed_to_send_gives_its_count_back() {
        let mut ledger = ReplyLedger::default();
        assert!(ledger.is_new(&reply("pending-1", "pending-1")));
        assert!(ledger.retire("pending-1"));
        assert!(!ledger.retire("pending-1"));
    }

    #[test]
    fn an_edit_to_a_reply_is_not_another_reply() {
        let mut ledger = ReplyLedger::default();
        assert!(ledger.is_new(&reply("r1", "")));
        let mut edited = reply("r2", "");
        edited.edit_at = 300;
        assert!(!ledger.is_new(&edited), "edited before we ever saw it");
    }
}
