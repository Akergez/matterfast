use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::app::App;

/// `GET /posts/{post}`: one post by its id, which is all a link to a message
/// or a search hit names. A client going to one asks for it and for the
/// pages either side of it.
pub(crate) async fn get_post(
    State(app): State<Arc<App>>,
    Path(post_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let db = app.db.lock().unwrap();
    db.posts
        .iter()
        .map(|post| &post.v)
        .find(|post| post["id"] == post_id.as_str())
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
