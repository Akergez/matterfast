use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use serde_json::{json, Value};

use crate::app::App;
use crate::ids::id;

pub(super) async fn handle_socket(socket: WebSocket, app: Arc<App>) {
    use futures_util::{SinkExt, StreamExt};
    let (mut sink, mut source) = socket.split();

    let conn_id = id("w", app.conn.fetch_add(1, Ordering::SeqCst));
    let mut events = app.events.subscribe();

    // `hello` is always sequence 0 of a stream.
    app.seq.store(0, Ordering::SeqCst);
    let hello = json!({
        "event": "hello",
        "data": {"connection_id": conn_id, "server_version": "11.11.0.test"},
        "broadcast": {},
        "seq": app.seq.fetch_add(1, Ordering::SeqCst),
    });
    if sink
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            event = events.recv() => {
                let Ok(event) = event else { continue };
                if sink.send(Message::Text(event.to_string().into())).await.is_err() {
                    return;
                }
            }
            incoming = source.next() => {
                let Some(Ok(msg)) = incoming else { return };
                let Message::Text(text) = msg else { continue };
                let Ok(req) = serde_json::from_str::<Value>(&text) else { continue };
                // The client pings every 30s and closes the socket if the reply
                // does not arrive before the next tick.
                if req["action"] == "ping" {
                    let reply = json!({
                        "status": "OK",
                        "seq_reply": req["seq"],
                        "data": {"text": "pong", "version": "11.11.0"},
                    });
                    if sink.send(Message::Text(reply.to_string().into())).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}
