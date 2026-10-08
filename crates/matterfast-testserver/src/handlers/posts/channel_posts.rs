use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};

use crate::app::App;

pub(crate) async fn channel_posts(
    State(app): State<Arc<App>>,
    Path(channel_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let since: i64 = q.get("since").and_then(|s| s.parse().ok()).unwrap_or(0);
    let crt = q.get("collapsedThreads").map(String::as_str) == Some("true");
    let before = q.get("before").map(String::as_str);
    let page = q.get("page").and_then(|s| s.parse().ok()).unwrap_or(0usize);
    let per_page = q
        .get("per_page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(60usize)
        .clamp(1, 200);

    let db = app.db.lock().unwrap();
    let mut matched: Vec<Value> = db
        .posts
        .iter()
        .map(|p| &p.v)
        .filter(|p| {
            p["channel_id"] == channel_id.as_str()
                && p["update_at"].as_i64().unwrap_or(0) > since
                // Under CRT, replies live in threads and never in the channel feed.
                && (!crt || p["root_id"].as_str().unwrap_or_default().is_empty())
        })
        .cloned()
        .collect();
    matched.sort_by_key(|p| -p["create_at"].as_i64().unwrap_or(0));

    // `after=<post>` is the page that follows a post, towards the present:
    // what a client asks for to walk forward from a block of history. The
    // list is newest first, so that page ends just short of the post.
    let after = q
        .get("after")
        .and_then(|id| matched.iter().position(|post| post["id"] == id.as_str()));
    let (start, end) = match after {
        Some(position) => (position.saturating_sub(per_page), position),
        None => {
            let start = before
                .and_then(|id| matched.iter().position(|post| post["id"] == id))
                .map_or_else(|| page.saturating_mul(per_page), |position| position + 1)
                .min(matched.len());
            (start, start.saturating_add(per_page).min(matched.len()))
        }
    };
    let page = &matched[start..end];
    let order: Vec<String> = page
        .iter()
        .filter_map(|post| post["id"].as_str().map(str::to_string))
        .collect();
    let posts: serde_json::Map<String, Value> = page
        .iter()
        .filter_map(|post| Some((post["id"].as_str()?.to_string(), post.clone())))
        .collect();
    Json(json!({
        "order": order,
        "posts": posts,
        "next_post_id": if start > 0 { "newer" } else { "" },
        "prev_post_id": if end < matched.len() { "older" } else { "" },
        "first_inaccessible_post_time": 0,
    }))
}
