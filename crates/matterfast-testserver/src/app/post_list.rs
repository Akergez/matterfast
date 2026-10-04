use serde_json::{json, Value};

use super::state::App;

pub(crate) fn post_list(app: &App, filter: impl Fn(&Value) -> bool) -> Value {
    let db = app.db.lock().unwrap();
    let mut matched: Vec<&Value> = db
        .posts
        .iter()
        .map(|p| &p.v)
        .filter(|p| filter(p))
        .collect();
    matched.sort_by_key(|p| -p["create_at"].as_i64().unwrap_or(0));

    let order: Vec<String> = matched
        .iter()
        .map(|p| p["id"].as_str().unwrap_or_default().to_string())
        .collect();
    let mut posts = serde_json::Map::new();
    for p in matched {
        posts.insert(p["id"].as_str().unwrap_or_default().to_string(), p.clone());
    }
    json!({
        "order": order,
        "posts": posts,
        // Empty both ways means "this page is the whole channel".
        "next_post_id": "",
        "prev_post_id": "",
        "first_inaccessible_post_time": 0,
    })
}
