use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::app::{apply_reaction, App};
use crate::clock::now;
use crate::constants::ME;

pub(crate) async fn add_reaction(
    State(app): State<Arc<App>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let post_id = body["post_id"].as_str().unwrap_or_default().to_string();
    let emoji = body["emoji_name"].as_str().unwrap_or_default().to_string();
    let reaction = json!({
        "user_id": ME, "post_id": post_id, "emoji_name": emoji,
        "create_at": now(), "update_at": now(), "delete_at": 0,
        "channel_id": "",
    });
    apply_reaction(&app, &post_id, &reaction, true);
    app.emit(
        "reaction_added",
        // Again: a JSON string, not an object.
        json!({ "reaction": serde_json::to_string(&reaction).unwrap() }),
        json!({ "channel_id": "" }),
    );
    Json(reaction)
}
