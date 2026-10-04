use serde_json::{json, Value};

use super::state::App;

pub(crate) fn apply_reaction(app: &App, post_id: &str, reaction: &Value, add: bool) {
    let mut db = app.db.lock().unwrap();
    let Some(post) = db.posts.iter_mut().find(|p| p.v["id"] == post_id) else {
        return;
    };
    let list = post.v["metadata"]["reactions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut list: Vec<Value> = list;
    if add {
        list.push(reaction.clone());
    } else {
        list.retain(|r| {
            !(r["user_id"] == reaction["user_id"] && r["emoji_name"] == reaction["emoji_name"])
        });
    }
    post.v["has_reactions"] = json!(!list.is_empty());
    post.v["metadata"]["reactions"] = json!(list);
}
