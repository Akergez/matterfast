use mattermost_api::models::*;

/// Posts for one channel.
///
/// A single block, and it has to stay one without holes: what is drawn is
/// read as everything said between its first post and its last, and the only
/// ways to more are the pages before the first and after the last. Two things
/// would break that, and each has its own way in:
///
/// - a page fetched around some post — a search hit, a link, where reading
///   stopped — which may be nowhere near what is held: [`land`](Self::land)
///   joins it on when the two share a post and puts it in place of what was
///   held when they do not;
/// - a message arriving live while the block stops short of the present
///   (`at_latest` is false): [`live`](Self::live) leaves it out, and it
///   comes with the page after the last post like everything else in between.
///
/// Without them a jump to a message of three months ago left it and its
/// neighbours on top of today's page with nothing in between and nothing to
/// fetch what was missing: scrolling asks for what is before the first post
/// and after the last, never for the middle.
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
    /// The newest message announced while the block stopped short of the
    /// present: not part of what is drawn as the conversation, but it is the
    /// last thing said there, which the list of conversations shows.
    pub ahead: Option<Post>,
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
            ahead: None,
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
            ahead: None,
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

    fn holds(&self, post: &Post) -> bool {
        self.posts.iter().any(|held| {
            held.id == post.id
                || (!post.pending_post_id.is_empty()
                    && (held.id == post.pending_post_id
                        || held.pending_post_id == post.pending_post_id))
        })
    }

    /// Takes a post the server has just announced. A change to one that is
    /// held always goes in. A new one goes in only when the block runs to
    /// the present: after a block that stops short of it there would be a
    /// hole, and the post will come with the page that closes the distance.
    pub fn live(&mut self, post: Post) {
        if self.at_latest || self.posts.is_empty() || self.holds(&post) {
            self.upsert(post);
            return;
        }
        // Still the last thing said, wherever the block is.
        let later = self.ahead.as_ref().is_none_or(|ahead| {
            ahead.id == post.id || ahead.create_at <= post.create_at
        });
        if later {
            self.ahead = Some(post);
        }
    }

    /// The last thing known to have been said in the channel: the end of the
    /// block, or a message announced since that the block has not reached.
    pub fn newest(&self) -> Option<&Post> {
        let held = self.posts.last();
        match (&self.ahead, held) {
            (Some(ahead), Some(held)) if held.create_at >= ahead.create_at => Some(held),
            (Some(ahead), _) => Some(ahead),
            (None, held) => held,
        }
    }

    /// Takes a block of posts fetched from somewhere in the channel's
    /// history, which says of itself whether it reaches the channel's first
    /// post and its newest.
    ///
    /// A block that shares a post with what is held continues it, and the two
    /// become one. One that shares nothing is somewhere else in the history,
    /// with posts in between that nobody has: it takes the place of what was
    /// held, which can be fetched again the same way it was the first time.
    pub fn land(&mut self, block: Vec<Post>, at_oldest: bool, at_latest: bool) {
        if block.is_empty() {
            return;
        }
        let joins = block.iter().any(|post| self.holds(post));
        if !joins {
            self.posts.clear();
            self.at_oldest = false;
            self.at_latest = false;
            self.last_fetched_at = 0;
        }
        for post in block {
            self.upsert(post);
        }
        self.at_oldest |= at_oldest;
        self.at_latest |= at_latest;
    }

    /// Soft-deletes a post, matching the server's own semantics.
    pub fn remove(&mut self, post_id: &str) {
        self.posts.retain(|p| p.id != post_id);
        if self.ahead.as_ref().is_some_and(|ahead| ahead.id == post_id) {
            self.ahead = None;
        }
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

    fn ids(feed: &ChannelFeed) -> Vec<&str> {
        feed.posts.iter().map(|p| p.id.as_str()).collect()
    }

    /// Today's page of a channel: posts 10 to 12, up to the present.
    fn today() -> ChannelFeed {
        let mut feed = ChannelFeed { at_latest: true, ..Default::default() };
        for n in 10..=12 {
            feed.upsert(post(&n.to_string(), n * 100));
        }
        feed
    }

    #[test]
    fn a_block_from_elsewhere_in_the_history_takes_the_place_of_what_was_held() {
        // A search hit far back: posts 1 to 3, with 4 to 9 in nobody's hands.
        let mut feed = today();
        let block = (1..=3).map(|n| post(&n.to_string(), n * 100)).collect();
        feed.land(block, false, false);

        assert_eq!(ids(&feed), ["1", "2", "3"], "no hole between 3 and 10");
        assert!(!feed.at_latest, "and the way on from 3 is the page after it");
        assert_eq!(feed.last_fetched_at, 300);
    }

    #[test]
    fn a_block_that_shares_a_post_with_what_is_held_continues_it() {
        let mut feed = today();
        let block = (8..=10).map(|n| post(&n.to_string(), n * 100)).collect();
        feed.land(block, true, false);

        assert_eq!(ids(&feed), ["8", "9", "10", "11", "12"]);
        assert!(feed.at_latest, "it still runs to the present");
        assert!(feed.at_oldest, "and now back to the beginning");
    }

    #[test]
    fn an_empty_block_changes_nothing() {
        let mut feed = today();
        feed.land(Vec::new(), true, true);
        assert_eq!(ids(&feed), ["10", "11", "12"]);
        assert!(!feed.at_oldest);
    }

    #[test]
    fn a_new_message_is_left_out_of_a_block_that_stops_short_of_the_present() {
        let mut feed = today();
        feed.land(vec![post("1", 100), post("2", 200)], false, false);

        feed.live(post("13", 1300));
        assert_eq!(ids(&feed), ["1", "2"], "it comes with the page after 2");
        assert_eq!(feed.newest().map(|p| p.id.as_str()), Some("13"), "but it is the last said");

        // A change to a post that is held is not a new message.
        let mut edited = post("2", 200);
        edited.message = "edited".into();
        feed.live(edited);
        assert_eq!(feed.posts[1].message, "edited");
    }

    #[test]
    fn a_new_message_goes_into_a_block_that_runs_to_the_present() {
        let mut feed = today();
        feed.live(post("13", 1300));
        assert_eq!(ids(&feed), ["10", "11", "12", "13"]);

        // And into a channel nothing has been said in yet.
        let mut empty = ChannelFeed::default();
        empty.live(post("1", 100));
        assert_eq!(ids(&empty), ["1"]);
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
