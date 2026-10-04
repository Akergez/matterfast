use mattermost_api::models::Post;

/// Whether a post belongs in the feed on screen at all — the question both
/// "should this row be there" and "should sending it move the reader" turn on.
pub(crate) fn feed_shows(showing: Option<&str>, post: &Post, crt: bool) -> bool {
    showing == Some(post.channel_id.as_str())
        && !post.is_deleted()
        && !post.is_system()
        && !(crt && post.is_reply())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post() -> Post {
        Post {
            id: "p1".into(),
            channel_id: "c1".into(),
            user_id: "u1".into(),
            create_at: 1_000,
            ..Default::default()
        }
    }

    #[test]
    fn the_feed_only_shows_posts_that_belong_in_it() {
        let mine = post();
        assert!(feed_shows(Some("c1"), &mine, true));
        assert!(!feed_shows(Some("c2"), &mine, true));
        assert!(!feed_shows(None, &mine, true));
    }

    #[test]
    fn a_reply_is_the_feeds_business_only_without_collapsed_threads() {
        // A reply is a thread's business while CRT is on, and the channel's
        // once it is off — sending one must move the reader in exactly the
        // second case.
        let reply = Post {
            root_id: "root".into(),
            ..post()
        };
        assert!(!feed_shows(Some("c1"), &reply, true));
        assert!(feed_shows(Some("c1"), &reply, false));
    }
}
