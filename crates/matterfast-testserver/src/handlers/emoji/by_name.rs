use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::names::EMOJI;
use super::record::emoji;

pub(crate) async fn emoji_by_name(Path(name): Path<String>) -> Response {
    if EMOJI.contains(&name.as_str()) {
        Json(emoji(&name)).into_response()
    } else {
        axum::http::StatusCode::NOT_FOUND.into_response()
    }
}
