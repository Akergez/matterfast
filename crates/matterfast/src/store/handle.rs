use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;

/// One database per server, holding that server's cached rows.
///
/// Cheap to clone: every clone shares the one connection.
#[derive(Clone)]
pub struct Store {
    // `rusqlite::Connection` is `Send` but not `Sync`, so it needs a lock to be
    // shared, and the lock has to be an async one: these methods are awaited on
    // a Tokio worker via `crate::runtime::spawn`, and a `std::sync::Mutex`
    // guard held across the call would make the future `!Send`.
    //
    // One long-lived connection rather than open-per-operation because opening
    // is the expensive part — a fresh file handle, header read and page-cache
    // per websocket post is real work to avoid a lock that is uncontended in
    // practice (one app, one store, writes measured in microseconds).
    //
    // ponytail: the blocking SQLite call runs on a Tokio worker rather than
    // `spawn_blocking`. Writes here are a handful of small rows; move to
    // `spawn_blocking` if a sync ever imports enough history to be felt.
    pub(super) conn: Arc<Mutex<Connection>>,
}
