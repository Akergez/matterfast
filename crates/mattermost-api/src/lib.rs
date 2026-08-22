//! An async Mattermost client: REST v4 plus the reliable WebSocket API.
//!
//! The shapes here were derived by reading `mattermost/mattermost` at
//! `server/public/model/*.go` and `webapp/platform/client/src/`, not from the
//! published OpenAPI document — several fields the clients depend on are
//! undocumented or documented incorrectly (see the notes on
//! [`ws::event`] about double-encoded payloads).
//!
//! # Quick start
//!
//! ```no_run
//! use mattermost_api::{Client, ws::{WebSocket, WsUpdate, Event}};
//!
//! # async fn run() -> mattermost_api::Result<()> {
//! let client = Client::new("https://mm.example.com")?;
//! let me = client.login("alice", "hunter2", None).await?;
//!
//! let ws = WebSocket::connect(client.websocket_url(), client.token().unwrap());
//! let mut rx = ws.subscribe();
//!
//! while let Ok(update) = rx.recv().await {
//!     match update {
//!         WsUpdate::Event(Event::Posted(p)) => {
//!             println!("{}: {}", p.sender_name, p.post.message);
//!         }
//!         WsUpdate::MissedMessages => { /* full REST resync */ }
//!         _ => {}
//!     }
//! }
//! # let _ = me;
//! # Ok(())
//! # }
//! ```
//!
//! # Startup sequence
//!
//! [`bootstrap`] documents and implements the order the official clients use.

pub mod bootstrap;
pub mod error;
pub mod models;
pub mod rest;
pub mod tls;
pub mod ws;

pub use error::{AppError, Error, Result};
pub use models::*;
pub use rest::Client;
pub use ws::{Event, WebSocket, WsUpdate};

/// Mattermost ids are 26-character base32-ish strings.
pub const ID_LEN: usize = 26;

/// True if `s` looks like a Mattermost id. Several plugin routes (Calls in
/// particular) 404 rather than 400 on a malformed id, so it is worth checking.
pub fn is_valid_id(s: &str) -> bool {
    s.len() == ID_LEN
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    #[test]
    fn id_validation() {
        assert!(super::is_valid_id("kj3n8x2q1w9e7r5t4y6u8i0o1p"));
        assert!(!super::is_valid_id("too-short"));
        assert!(!super::is_valid_id("KJ3N8X2Q1W9E7R5T4Y6U8I0O1P"));
    }
}
