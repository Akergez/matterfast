//! Where this user's files live.
//!
//! The XDG base directories, resolved once. They used to come from GLib, which
//! reads the environment on first use and never again; sessions, the snapshot
//! and the message store were all written under those answers, so these have
//! to be the same directories byte for byte or an upgrade loses every one of
//! them.

mod cache_dir;
mod config_dir;
mod data_dir;
mod resolve;
mod resolve_from;

pub use cache_dir::cache_dir;
pub use config_dir::config_dir;
pub use data_dir::data_dir;
