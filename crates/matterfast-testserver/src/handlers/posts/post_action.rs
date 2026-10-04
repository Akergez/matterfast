use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::app::App;
use crate::clock::now;

/// A button or a menu on a card. The real server forwards this to the
/// integration and applies its answer; this plays both parts, replacing the
/// buttons with what was decided and announcing the edit.
pub(crate) async fn post_action(
    State(app): State<Arc<App>>,
    Path((post_id, action_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let selected = body["selected_option"].as_str().unwrap_or_default();
    let outcome = match (action_id.as_str(), selected) {
        (action, "") => format!("anton pressed `{action}`."),
        (action, selected) => format!("anton set `{action}` to `{selected}`."),
    };
    let edited = {
        let mut db = app.db.lock().unwrap();
        let Some(post) = db.posts.iter_mut().find(|p| p.v["id"] == post_id) else {
            return Json(json!({ "status": "OK" }));
        };
        let card = &mut post.v["props"]["attachments"][0];
        card["actions"] = Value::Null;
        card["text"] = json!(outcome);
        post.v["update_at"] = json!(now());
        post.v.clone()
    };
    app.emit(
        "post_edited",
        json!({ "post": serde_json::to_string(&edited).unwrap() }),
        json!({ "channel_id": edited["channel_id"] }),
    );
    Json(json!({ "status": "OK", "trigger_id": "" }))
}
