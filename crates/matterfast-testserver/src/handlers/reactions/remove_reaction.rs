use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::json;

use crate::app::{apply_reaction, App};
use crate::clock::now;

pub(crate) async fn remove_reaction(
    State(app): State<Arc<App>>,
    Path((user_id, post_id, emoji)): Path<(String, String, String)>,
) -> StatusCode {
    let reaction = json!({
        "user_id": user_id, "post_id": post_id, "emoji_name": emoji,
        "create_at": 0, "update_at": 0, "delete_at": now(), "channel_id": "",
    });
    apply_reaction(&app, &post_id, &reaction, false);
    app.emit(
        "reaction_removed",
        json!({ "reaction": serde_json::to_string(&reaction).unwrap() }),
        json!({ "channel_id": "" }),
    );
    StatusCode::OK
}
