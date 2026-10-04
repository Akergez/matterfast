use std::sync::atomic::Ordering;

use serde_json::{json, Value};

use super::state::App;

impl App {
    /// Broadcasts an event with the server's own monotonic sequence, which the
    /// client validates strictly.
    pub(crate) fn emit(&self, event: &str, data: Value, broadcast_: Value) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let _ = self.events.send(json!({
            "event": event,
            "data": data,
            "broadcast": broadcast_,
            "seq": seq,
        }));
    }
}
