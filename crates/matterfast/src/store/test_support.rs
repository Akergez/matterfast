use std::fs;
use std::path::PathBuf;

use mattermost_api::models::{Millis, Post};

/// A directory of our own per test. `Store::open_at` exists for this: the
/// tests must not read, write or delete anything in the real user data dir.
pub(super) fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("matterfast-store-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

pub(super) fn post(id: &str, channel: &str, at: Millis) -> Post {
    Post {
        id: id.into(),
        channel_id: channel.into(),
        create_at: at,
        message: format!("message {id}"),
        ..Default::default()
    }
}
