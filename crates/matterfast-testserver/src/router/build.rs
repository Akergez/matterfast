use std::sync::Arc;

use axum::middleware;
use axum::Router;

use super::api::api;
use super::request_log::request_log;
use super::zed::zed;
use crate::app::App;

pub(crate) fn router(app: Arc<App>) -> Router {
    Router::new()
        .nest("/api/v4", api())
        .with_state(app)
        .nest("/zed", zed())
        .layer(middleware::from_fn(request_log))
}
