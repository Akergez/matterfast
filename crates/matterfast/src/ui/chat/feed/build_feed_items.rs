use std::rc::Rc;

use gpui_kit::SharedString;
use mattermost_api::models::{Millis, Post};

use super::feed_item::FeedItem;
use crate::state::AppState;
use crate::timefmt::format_day;
use crate::ui::message;

fn flush_system(run: &mut Vec<Post>, items: &mut Vec<FeedItem>, st: &AppState) {
    if run.is_empty() {
        return;
    }
    items.push(FeedItem::System {
        key: run[0].id.clone(),
        lines: Rc::new(
            message::system_lines(run, st)
                .into_iter()
                .map(SharedString::from)
                .collect(),
        ),
    });
    run.clear();
}

pub(crate) fn build_feed_items(
    posts: &[Post],
    st: &AppState,
    crt: bool,
    unread_since: Option<Millis>,
    at_oldest_title: Option<&str>,
) -> Vec<FeedItem> {
    let mut items = Vec::with_capacity(posts.len() + 4);
    if let Some(title) = at_oldest_title.filter(|_| !posts.is_empty()) {
        items.push(FeedItem::Start(title.to_string()));
    }
    if posts.is_empty() {
        items.push(FeedItem::Empty);
        return items;
    }

    let mut unread_drawn = false;
    let mut last_author: Option<String> = None;
    let mut last_at: Millis = 0;
    let mut last_day: Option<String> = None;
    let mut system_run: Vec<Post> = Vec::new();

    for post in posts {
        if post.is_deleted() || (crt && post.is_reply()) {
            continue;
        }
        if unread_since.is_some_and(|at| post.create_at > at) && !unread_drawn {
            flush_system(&mut system_run, &mut items, st);
            items.push(FeedItem::Unread);
            unread_drawn = true;
        }

        let day = format_day(post.create_at);
        if last_day.as_deref() != Some(day.as_str()) {
            flush_system(&mut system_run, &mut items, st);
            items.push(FeedItem::Day(day.clone()));
            last_day = Some(day);
            last_author = None;
        }

        if post.is_system() {
            system_run.push(post.clone());
            last_author = None;
            continue;
        }

        flush_system(&mut system_run, &mut items, st);
        let author = st.author_name(post);
        let grouped = last_author.as_deref() == Some(author.as_str())
            && post.create_at.saturating_sub(last_at) < message::GROUPING_WINDOW_MS;
        items.push(FeedItem::Post {
            post: Rc::new(post.clone()),
            grouped,
            body: message::message_markdown(&post.message, st).into(),
        });
        last_author = Some(author);
        last_at = post.create_at;
    }
    flush_system(&mut system_run, &mut items, st);
    items
}
