use mattermost_api::models::{ClientConfig, Millis, Post, User};
use mattermost_api::Client;

use super::build_feed_items::build_feed_items;
use super::feed_item::FeedItem;
use super::splice_plan::splice_plan;
use crate::state::AppState;
use crate::timefmt::format_day;
use crate::ui::message;

fn state_with(users: &[User]) -> AppState {
    let client = Client::new("http://x.test").unwrap();
    let mut app = AppState::new(client, User::default(), ClientConfig::default(), false);
    for user in users {
        app.users.insert(user.id.clone(), user.clone());
    }
    app
}

fn user(id: &str, username: &str) -> User {
    User {
        id: id.into(),
        username: username.into(),
        ..Default::default()
    }
}

fn post(user_id: &str, at: Millis) -> Post {
    Post {
        id: format!("p{at}"),
        user_id: user_id.into(),
        create_at: at,
        ..Default::default()
    }
}

/// Whether `post` groups with the post drawn immediately before it: same
/// author, same day, close enough in time. Author is compared by name rather
/// than user id because a webhook can post under a different name per message
/// with the same id.
fn groups_with(prev: Option<&Post>, post: &Post, st: &AppState) -> bool {
    let Some(prev) = prev else {
        return false;
    };
    format_day(post.create_at) == format_day(prev.create_at)
        && st.author_name(post) == st.author_name(prev)
        && post.create_at.saturating_sub(prev.create_at) < message::GROUPING_WINDOW_MS
}

#[test]
fn same_author_within_the_window_groups() {
    let state = state_with(&[user("u1", "anna")]);
    let prev = post("u1", 1_000);
    let next = post("u1", 1_000 + message::GROUPING_WINDOW_MS - 1);
    assert!(groups_with(Some(&prev), &next, &state));
}

#[test]
fn a_gap_past_the_window_does_not_group() {
    let state = state_with(&[user("u1", "anna")]);
    let prev = post("u1", 1_000);
    let next = post("u1", 1_000 + message::GROUPING_WINDOW_MS);
    assert!(!groups_with(Some(&prev), &next, &state));
}

#[test]
fn a_different_author_never_groups_even_seconds_apart() {
    let state = state_with(&[user("u1", "anna"), user("u2", "bob")]);
    let prev = post("u1", 1_000);
    let next = post("u2", 1_001);
    assert!(!groups_with(Some(&prev), &next, &state));
}

#[test]
fn nothing_before_it_never_groups() {
    let state = state_with(&[]);
    let next = post("u1", 1_000);
    assert!(!groups_with(None, &next, &state));
}

fn keys(feed: &[FeedItem]) -> Vec<String> {
    feed.iter().map(FeedItem::key).collect()
}

/// Posts a few seconds apart on one day, so the only rows are the day,
/// the posts, and whatever the test adds.
fn posts(ids: std::ops::Range<i64>) -> Vec<Post> {
    ids.map(|n| Post {
        id: format!("p{n}"),
        user_id: "u1".into(),
        create_at: 1_700_000_000_000 + n * 1_000,
        ..Default::default()
    })
    .collect()
}

#[test]
fn the_feed_groups_and_separates_the_way_it_reads() {
    let state = state_with(&[user("u1", "anna")]);
    let feed = build_feed_items(&posts(0..3), &state, false, None, Some("Town Square"));
    assert!(matches!(feed[0], FeedItem::Start(_)));
    assert!(matches!(feed[1], FeedItem::Day(_)));
    // The first of a run names its author; the rest follow it.
    let grouped: Vec<bool> = feed
        .iter()
        .filter_map(|item| match item {
            FeedItem::Post { grouped, .. } => Some(*grouped),
            _ => None,
        })
        .collect();
    assert_eq!(grouped, [false, true, true]);

    // Nothing at all is its own page, not a start label over a void.
    let empty = build_feed_items(&[], &state, false, None, Some("Town Square"));
    assert!(matches!(empty.as_slice(), [FeedItem::Empty]));
}

#[test]
fn the_unread_line_goes_where_reading_stopped() {
    let state = state_with(&[user("u1", "anna")]);
    let posts = posts(0..4);
    let feed = build_feed_items(&posts, &state, false, Some(posts[1].create_at), None);
    let at = feed
        .iter()
        .position(|item| matches!(item, FeedItem::Unread))
        .expect("an unread line");
    assert_eq!(feed[at + 1].post_id(), Some("p2"));
    // After the line the group starts over only if the author changed —
    // the line itself is not an author.
    assert_eq!(feed[at - 1].post_id(), Some("p1"));
}

#[test]
fn replies_stay_out_of_the_feed_under_collapsed_threads() {
    let state = state_with(&[user("u1", "anna")]);
    let mut posts = posts(0..2);
    posts[1].root_id = "p0".into();
    assert_eq!(
        build_feed_items(&posts, &state, true, None, None)
            .iter()
            .filter(|item| item.post_id().is_some())
            .count(),
        1
    );
    assert_eq!(
        build_feed_items(&posts, &state, false, None, None)
            .iter()
            .filter(|item| item.post_id().is_some())
            .count(),
        2
    );
}

#[test]
fn an_older_page_is_a_splice_at_the_top() {
    let state = state_with(&[user("u1", "anna")]);
    let before = keys(&build_feed_items(&posts(5..10), &state, false, None, None));
    let after = keys(&build_feed_items(&posts(0..10), &state, false, None, None));
    // Everything from the first old post down is untouched; only the day
    // row and the new posts above it are replaced.
    let (removed, added) = splice_plan(&before, &after).unwrap();
    assert_eq!(removed, 1..1, "nothing already on screen is rebuilt");
    assert_eq!(added, 5);
}

#[test]
fn a_new_message_is_a_splice_at_the_bottom_of_a_built_feed() {
    let state = state_with(&[user("u1", "anna")]);
    let before = keys(&build_feed_items(&posts(0..5), &state, false, None, None));
    let after = keys(&build_feed_items(&posts(0..6), &state, false, None, None));
    assert_eq!(splice_plan(&before, &after), Some((6..6, 1)));
}

#[test]
fn an_edit_or_a_reaction_moves_nothing() {
    // The row is the same row; what it says is the drawing's business.
    let state = state_with(&[user("u1", "anna")]);
    let mut edited = posts(0..5);
    let before = keys(&build_feed_items(&edited, &state, false, None, None));
    edited[2].message = "changed".into();
    edited[2].edit_at = 5;
    let after = keys(&build_feed_items(&edited, &state, false, None, None));
    assert_eq!(splice_plan(&before, &after), None);
}
