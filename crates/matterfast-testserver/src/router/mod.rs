//! Which path goes to which handler.

mod api;
mod build;
mod request_log;
mod zed;

pub(crate) use build::router;
