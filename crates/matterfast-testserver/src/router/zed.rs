use axum::routing::get;
use axum::Router;

use crate::handlers::{zed_extension_download, zed_extensions};

/// The fake extension registry, as mounted under `/zed`.
pub(super) fn zed() -> Router {
    Router::new()
        .route("/extensions", get(zed_extensions))
        .route("/extensions/{id}/download", get(zed_extension_download))
}
