//! Fetching the people behind a page of posts.

use std::collections::HashSet;

use mattermost_api::models::{PostList, Status, User};

/// Post authors are not included in a feed response, so hydrate them before
/// rendering or every message reads "unknown". Presence comes along for the
/// ride: the same ids, and the avatars carry a status badge.
pub(crate) async fn hydrate_authors(
    client: &mattermost_api::Client,
    list: &PostList,
) -> (Vec<User>, Vec<Status>) {
    let ids: Vec<String> = list
        .posts
        .values()
        .map(|p| p.user_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|id| !id.is_empty())
        .collect();
    if ids.is_empty() {
        return (Vec::new(), Vec::new());
    }
    (
        client.users_by_ids(&ids).await.unwrap_or_default(),
        client.statuses_by_ids(&ids).await.unwrap_or_default(),
    )
}
