//! A fake Mattermost server, just real enough to exercise the client.
//!
//! It is not a mock in the unit-test sense — it holds actual state, assigns
//! real ids, and pushes the same websocket events the real server does,
//! including the double-encoded payloads (`posted.post` as a JSON *string*) and
//! the inconsistent key casing. Getting those wrong here would defeat the
//! purpose: the point is to catch the client mis-parsing what the server
//! actually sends.
//!
//! ```sh
//! cargo run -p matterfast-testserver           # listens on 127.0.0.1:8065
//! ```
//!
//! Log in with any username and password. A background "colleague" posts every
//! few seconds so live updates are visible without a second client.
//!
//! Under `/zed` it is also a fake of the one other server the client talks
//! to: the Zed editor's extension registry, from which themes are installed.
//! `MATTERFAST_THEMES_API=http://127.0.0.1:8065/zed` points the client at it,
//! so installing a theme can be tested with no network.

mod app;
mod clock;
mod constants;
mod handlers;
mod ids;
mod model;
mod router;
mod seed;

use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let (events, _) = broadcast::channel(256);
    let app = Arc::new(app::App {
        db: Mutex::new(app::Db::default()),
        events,
        seq: AtomicI64::new(0),
        conn: AtomicI64::new(1),
    });
    seed::seed(&app);
    tokio::spawn(seed::colleague(app.clone()));

    let address =
        std::env::var("MM_TESTSERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:8065".to_string());
    let listener = tokio::net::TcpListener::bind(&address).await.unwrap();
    println!("fake Mattermost on http://{address} — any username and password will do");
    axum::serve(listener, router::router(app)).await.unwrap();
}
