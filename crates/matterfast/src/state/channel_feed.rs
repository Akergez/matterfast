use mattermost_api::models::*;

/// Posts for one channel.
///
/// Deliberately a single contiguous block for now. The moment we add
/// permalink jumps or "jump to first unread", this has to become a list of
/// blocks with `recent` / `oldest` flags, merged on overlap — otherwise a jump
/// into old history silently appends unrelated posts to the live feed. Both
/// official clients learned this the hard way; the merge rule is that two
/// blocks join when the older one's newest post is not older than the newer
/// one's oldest post.
#[derive(Debug, Default)]
pub struct ChannelFeed {
    /// Oldest first — the order a chat renders in.
    pub posts: Vec<Post>,
    /// True when the newest post here is the newest post in the channel.
    pub at_latest: bool,
    /// True when this block reaches the beginning of the channel.
    pub at_oldest: bool,
    /// `create_at` of the newest post we hold; the `since` watermark.
    pub last_fetched_at: Millis,
}

impl ChannelFeed {
    pub fn from_list(list: &PostList) -> Self {
        let mut posts: Vec<Post> = list.chronological().into_iter().cloned().collect();
        // `order` is only newest-first for *channel* feeds. `/posts/{id}/thread`
        // applies no ORDER BY at all unless the request names a direction, so
        // its order is whatever the query plan happened to yield — reversing
        // that produces a scrambled thread. Sorting here rather than in the
        // thread panel also keeps `upsert`'s invariant: it binary-walks this
        // vec assuming it is sorted, so an unsorted feed misplaces every
        // websocket reply too.
        //
        // The sort is stable, so posts sharing a millisecond keep the order the
        // server listed them in, and the panel still hoists the root to the top.
        posts.sort_by_key(|p| p.create_at);
        let last_fetched_at = posts.last().map(|p| p.create_at).unwrap_or(0);
        ChannelFeed {
            posts,
            at_latest: list.is_recent(),
            at_oldest: list.is_oldest(),
            last_fetched_at,
        }
    }

    /// Builds a feed from posts that are already in order — the cache stores
    /// them the way this holds them, so there is nothing to sort or reverse.
    ///
    /// `at_latest` is false: a restored feed is a snapshot from last time and
    /// almost certainly has messages after it, which is exactly what stops the
    /// view claiming it is up to date before the network says so.
    pub fn from_posts(posts: Vec<Post>) -> Self {
        let last_fetched_at = posts.last().map(|p| p.create_at).unwrap_or(0);
        ChannelFeed {
            posts,
            at_latest: false,
            at_oldest: false,
            last_fetched_at,
        }
    }

    /// Inserts or replaces a post, keeping chronological order.
    ///
    /// Used for both `posted` and `post_edited`: the server sends whole posts,
    /// never diffs.
    pub fn upsert(&mut self, post: Post) {
        if let Some(existing) = self.posts.iter_mut().find(|p| p.id == post.id) {
            *existing = post;
            return;
        }
        // Replace our optimistic copy, if this is the echo of our own send.
        if !post.pending_post_id.is_empty() {
            if let Some(pending) = self
                .posts
                .iter_mut()
                .find(|p| p.id == post.pending_post_id || p.pending_post_id == post.pending_post_id)
            {
                *pending = post;
                return;
            }
        }
        self.last_fetched_at = self.last_fetched_at.max(post.create_at);
        match self
            .posts
            .iter()
            .rposition(|p| p.create_at <= post.create_at)
        {
            Some(idx) => self.posts.insert(idx + 1, post),
            None => self.posts.insert(0, post),
        }
    }

    /// Soft-deletes a post, matching the server's own semantics.
    pub fn remove(&mut self, post_id: &str) {
        self.posts.retain(|p| p.id != post_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: &str, at: Millis) -> Post {
        Post {
            id: id.into(),
            create_at: at,
            ..Default::default()
        }
    }

    #[test]
    fn feed_keeps_posts_in_chronological_order() {
        let mut feed = ChannelFeed::default();
        feed.upsert(post("b", 200));
        feed.upsert(post("a", 100));
        feed.upsert(post("c", 300));
        let ids: Vec<&str> = feed.posts.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        assert_eq!(feed.last_fetched_at, 300);
    }

    #[test]
    fn editing_a_post_replaces_it_in_place() {
        let mut feed = ChannelFeed::default();
        feed.upsert(post("a", 100));
        feed.upsert(post("b", 200));
        let mut edited = post("a", 100);
        edited.message = "edited".into();
        feed.upsert(edited);
        assert_eq!(feed.posts.len(), 2);
        assert_eq!(feed.posts[0].message, "edited");
    }

    #[test]
    fn the_server_echo_replaces_our_optimistic_copy() {
        let mut feed = ChannelFeed::default();
        let mut optimistic = post("pending-1", 100);
        optimistic.pending_post_id = "pending-1".into();
        feed.upsert(optimistic);

        let mut confirmed = post("real-id", 101);
        confirmed.pending_post_id = "pending-1".into();
        feed.upsert(confirmed);

        assert_eq!(feed.posts.len(), 1, "the pending post should be replaced");
        assert_eq!(feed.posts[0].id, "real-id");
    }

    #[test]
    fn from_list_sorts_a_thread_the_server_left_unordered() {
        // What `/posts/{id}/thread` really returns: the requested post first,
        // then replies in no particular order.
        let mut list = PostList {
            order: vec!["root".into(), "r2".into(), "r3".into(), "r1".into()],
            ..Default::default()
        };
        list.posts.insert("root".into(), post("root", 100));
        list.posts.insert("r1".into(), post("r1", 200));
        list.posts.insert("r2".into(), post("r2", 300));
        list.posts.insert("r3".into(), post("r3", 400));

        let feed = ChannelFeed::from_list(&list);
        let ids: Vec<&str> = feed.posts.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["root", "r1", "r2", "r3"]);
        assert_eq!(
            feed.last_fetched_at, 400,
            "the watermark is the newest post"
        );
    }

    #[test]
    fn from_list_reverses_the_servers_newest_first_order() {
        let mut list = PostList {
            order: vec!["c".into(), "b".into(), "a".into()],
            ..Default::default()
        };
        list.posts.insert("a".into(), post("a", 100));
        list.posts.insert("b".into(), post("b", 200));
        list.posts.insert("c".into(), post("c", 300));

        let feed = ChannelFeed::from_list(&list);
        let ids: Vec<&str> = feed.posts.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        assert!(feed.at_latest, "next_post_id was empty");
        assert!(feed.at_oldest, "prev_post_id was empty");
    }
}
