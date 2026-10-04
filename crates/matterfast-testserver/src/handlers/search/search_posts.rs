use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::Value;

use super::parse_search::parse_search;
use crate::app::{post_list, App};

pub(crate) async fn search_posts(
    State(app): State<Arc<App>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let terms = parse_search(body["terms"].as_str().unwrap_or_default());
    let any_word = body["is_or_search"].as_bool().unwrap_or(false);
    let offset = body["time_zone_offset"].as_i64().unwrap_or(0);
    let mut list = post_list(&app, |post| terms.matches(post, any_word, offset));

    // One page of it, newest first, as the real server cuts it.
    let per_page = body["per_page"].as_u64().unwrap_or(60).max(1) as usize;
    let page = body["page"].as_u64().unwrap_or(0) as usize;
    let order: Vec<Value> = list["order"]
        .as_array()
        .map(|order| order.iter().skip(page * per_page).take(per_page).cloned().collect())
        .unwrap_or_default();
    if let Some(posts) = list["posts"].as_object_mut() {
        posts.retain(|id, _| order.iter().any(|kept| kept == id));
    }
    // What a real server searching its own database was seen to say about
    // which words matched: every hit is named, and has `null` for its words —
    // a nil slice, as Go writes one. A client that expects a list there fails
    // on every search that finds something, which is how this was noticed.
    let matches: serde_json::Map<String, Value> = order
        .iter()
        .filter_map(|id| Some((id.as_str()?.to_string(), Value::Null)))
        .collect();
    list["order"] = order.into();
    list["matches"] = matches.into();
    Json(list)
}
