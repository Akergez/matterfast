use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app::App;
use crate::constants::ME;
use crate::model::user;

pub(crate) async fn login(State(app): State<Arc<App>>) -> Response {
    let _ = &app;
    let mut headers = HeaderMap::new();
    // The token comes back in a *header*, not the body.
    headers.insert("Token", "test-session-token".parse().unwrap());
    (headers, Json(user(ME))).into_response()
}
