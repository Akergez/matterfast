use std::sync::atomic::AtomicI64;
use std::sync::Mutex;

use serde_json::Value;
use tokio::sync::broadcast;

use super::db::Db;

pub(crate) struct App {
    pub(crate) db: Mutex<Db>,
    pub(crate) events: broadcast::Sender<Value>,
    pub(crate) seq: AtomicI64,
    pub(crate) conn: AtomicI64,
}
