use mattermost_api::models::Post;

use super::inbox::Inbox;
use super::inbox_row::InboxRow;
use super::target::Target;
use crate::state::AppState;

fn inbox_row_for(post: &Post, st: &AppState) -> InboxRow {
    InboxRow {
        user_id: post.user_id.clone(),
        author: st.author_name(post),
        channel: st
            .channel(&post.channel_id)
            .map(|c| st.channel_title(c))
            .unwrap_or_else(|| "unknown channel".into()),
        preview: crate::markdown::preview(&post.message).into(),
        at: post.create_at,
        counts: None,
        target: if post.is_reply() {
            Target::Thread {
                channel_id: post.channel_id.clone(),
                root_id: post.thread_root().to_string(),
            }
        } else {
            Target::Message {
                channel_id: post.channel_id.clone(),
                post_id: post.id.clone(),
            }
        },
    }
}

pub(super) fn build_inbox(st: &AppState) -> Inbox {
    let mentions = st
        .mentions
        .iter()
        .map(|post| inbox_row_for(post, st))
        .collect();

    // Saved posts: the ones you flagged, newest first. They are held as a set
    // of ids, so this walks what is loaded rather than fetching — anything not
    // in memory shows up as soon as its channel is opened.
    let mut saved: Vec<&Post> = st
        .feeds
        .values()
        .chain(st.threads.values())
        .flat_map(|feed| feed.posts.iter())
        .filter(|p| st.saved_posts.contains(&p.id))
        .collect();
    saved.sort_by(|a, b| b.create_at.cmp(&a.create_at).then(a.id.cmp(&b.id)));
    // The same post can be held by a channel feed and by its thread.
    saved.dedup_by(|a, b| a.id == b.id);
    let saved = saved
        .into_iter()
        .map(|post| inbox_row_for(post, st))
        .collect();

    let threads = st
        .thread_inbox
        .iter()
        .map(|thread| InboxRow {
            user_id: thread.post.user_id.clone(),
            author: st.author_name(&thread.post),
            channel: st
                .channel(&thread.post.channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default(),
            preview: crate::markdown::preview(&thread.post.message).into(),
            at: thread.last_reply_at.max(thread.post.create_at),
            counts: Some((
                thread.reply_count,
                thread.unread_replies,
                thread.unread_mentions,
            )),
            target: Target::Followed(thread.id.clone()),
        })
        .collect();

    Inbox {
        mentions,
        threads,
        saved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::models::{ClientConfig, User};
    use mattermost_api::Client;

    fn state() -> AppState {
        let client = Client::new("http://x.test").unwrap();
        let mut st = AppState::new(client, User::default(), ClientConfig::default(), false);
        st.users.insert(
            "u1".into(),
            User {
                id: "u1".into(),
                username: "anna".into(),
                ..Default::default()
            },
        );
        st
    }

    #[test]
    fn a_saved_post_held_twice_is_listed_once() {
        let mut st = state();
        let saved = Post {
            id: "p1".into(),
            user_id: "u1".into(),
            create_at: 10,
            channel_id: "c1".into(),
            message: "**keep** this".into(),
            ..Default::default()
        };
        st.saved_posts.insert("p1".into());
        st.feeds.insert(
            "c1".into(),
            crate::state::ChannelFeed::from_posts(vec![saved.clone()]),
        );
        st.threads
            .insert("p1".into(), crate::state::ChannelFeed::from_posts(vec![saved]));

        let inbox = build_inbox(&st);
        assert_eq!(inbox.saved.len(), 1);
        // A preview is read, not rendered: no Markdown markers in it.
        assert_eq!(inbox.saved[0].preview.as_ref(), "keep this");
        // Not a reply, so pressing it goes to the message.
        assert!(matches!(inbox.saved[0].target, Target::Message { .. }));
    }
}
