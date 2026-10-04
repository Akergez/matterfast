use std::rc::Rc;

use mattermost_api::models::{Millis, Post};

use super::thread_row::ThreadRow;
use crate::state::AppState;
use crate::ui::message;

/// A thread's rows: the root, the reply count, then the replies.
pub(super) fn build_thread_rows(
    mut posts: Vec<Post>,
    root_id: &str,
    st: &AppState,
) -> Vec<ThreadRow> {
    // The feed arrives sorted by `create_at` (see `ChannelFeed::from_list`,
    // which has to sort because the thread endpoint does not). That is still
    // not enough: two posts can share a millisecond on a busy server, and then
    // the conversation starts with a reply. So the root goes first, always.
    if let Some(index) = posts.iter().position(|p| p.id == root_id) {
        let root = posts.remove(index);
        posts.insert(0, root);
    }
    posts.retain(|post| !post.is_deleted());
    let reply_count = posts.len().saturating_sub(1) as i64;

    let mut rows = Vec::with_capacity(posts.len() + 1);
    let mut last_author: Option<String> = None;
    let mut last_at: Millis = 0;
    for (index, post) in posts.into_iter().enumerate() {
        if index == 1 && reply_count > 0 {
            rows.push(ThreadRow::Divider(format!(
                "{reply_count} {}",
                message::plural(reply_count, "reply", "replies")
            )));
            last_author = None;
        }
        let author = st.author_name(&post);
        let grouped = index != 0
            && last_author.as_deref() == Some(author.as_str())
            && post.create_at - last_at < message::GROUPING_WINDOW_MS;
        last_author = Some(author);
        last_at = post.create_at;
        rows.push(ThreadRow::Post {
            body: message::message_markdown(&post.message, st).into(),
            post: Rc::new(post),
            grouped,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::models::{ClientConfig, User};
    use mattermost_api::Client;

    fn state() -> AppState {
        let client = Client::new("http://x.test").unwrap();
        let mut st = AppState::new(client, User::default(), ClientConfig::default(), false);
        for (id, name) in [("u1", "anna"), ("u2", "bob")] {
            st.users.insert(
                id.into(),
                User {
                    id: id.into(),
                    username: name.into(),
                    ..Default::default()
                },
            );
        }
        st
    }

    fn post(id: &str, user: &str, at: Millis) -> Post {
        Post {
            id: id.into(),
            user_id: user.into(),
            create_at: at,
            ..Default::default()
        }
    }

    fn keys(rows: &[ThreadRow]) -> Vec<String> {
        rows.iter().map(ThreadRow::key).collect()
    }

    #[test]
    fn the_root_leads_the_thread_even_when_a_reply_shares_its_millisecond() {
        let st = state();
        // The server listed the reply first; both carry the same timestamp.
        let posts = vec![post("reply", "u2", 1_000), post("root", "u1", 1_000)];
        assert_eq!(
            keys(&build_thread_rows(posts, "root", &st)),
            ["post:root", "divider", "post:reply"]
        );
    }

    #[test]
    fn the_divider_counts_replies_and_is_absent_without_any() {
        let st = state();
        let rows = build_thread_rows(
            vec![
                post("root", "u1", 1),
                post("a", "u2", 2),
                post("b", "u2", 3),
            ],
            "root",
            &st,
        );
        assert!(matches!(&rows[1], ThreadRow::Divider(label) if label == "2 replies"));
        // The second reply follows the first; the first follows a divider and
        // so names its author again.
        let grouped: Vec<bool> = rows
            .iter()
            .filter_map(|row| match row {
                ThreadRow::Post { grouped, .. } => Some(*grouped),
                _ => None,
            })
            .collect();
        assert_eq!(grouped, [false, false, true]);

        let alone = build_thread_rows(vec![post("root", "u1", 1)], "root", &st);
        assert_eq!(keys(&alone), ["post:root"]);
    }

    #[test]
    fn a_deleted_reply_is_neither_drawn_nor_counted() {
        let st = state();
        let mut gone = post("gone", "u2", 2);
        gone.delete_at = 5;
        let rows = build_thread_rows(
            vec![post("root", "u1", 1), gone, post("kept", "u2", 3)],
            "root",
            &st,
        );
        assert_eq!(keys(&rows), ["post:root", "divider", "post:kept"]);
        assert!(matches!(&rows[1], ThreadRow::Divider(label) if label == "1 reply"));
    }
}
