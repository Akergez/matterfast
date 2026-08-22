//! Reliable WebSocket client for `/api/v4/websocket`.
//!
//! Implements the same contract the official clients do:
//!
//! * authenticate with `Authorization: Bearer` **on the upgrade request** — the
//!   in-band `authentication_challenge` works too, but binary frames are
//!   rejected on a not-yet-authenticated connection, which would break Calls
//!   signalling (SDP is sent as a binary msgpack frame);
//! * reconnect with `?connection_id=…&sequence_number=…` so the server replays
//!   its 128-event dead queue instead of us doing a full REST resync;
//! * detect a *failed* resume by comparing the `connection_id` in `hello`
//!   against the one we held, and surface it as [`WsUpdate::MissedMessages`];
//! * detect a mid-stream gap (`seq != expected`) and force a reconnect rather
//!   than trying to patch the hole in place;
//! * run a 30 s application-level ping, because a real close event can arrive
//!   minutes after the socket is actually dead.

pub mod event;

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

pub use event::{Broadcast, Data, Event, Posted, WsFrame, WsRequest};

use crate::error::{Error, Result};

/// Flat retry delay for the first [`MAX_FAILS`] attempts.
const MIN_RETRY: Duration = Duration::from_secs(3);
const MAX_RETRY: Duration = Duration::from_secs(300);
const MAX_FAILS: u32 = 7;
const JITTER_MS: u64 = 2000;
/// Application-level ping cadence (the server also sends protocol pings at 60s).
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// Close codes the server accepts in `disconnect_err_code` telemetry.
const CLOSE_PING_TIMEOUT: u16 = 4000;
const CLOSE_SEQUENCE_MISMATCH: u16 = 4001;

/// What the connection task reports to the application.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // Event is the hot path; boxing it would
                                     // cost an allocation per websocket frame.
pub enum WsUpdate {
    /// The socket is open and authenticated.
    ///
    /// `resumed` is true when the server recognised our `connection_id` and
    /// replayed whatever we missed — in that case **do not** run a REST resync.
    Connected {
        connection_id: String,
        resumed: bool,
    },
    /// The server could not replay: run the full gap-fill (teams, channels,
    /// categories, `posts?since=`, preferences, statuses).
    MissedMessages,
    Event(Event),
    Reply {
        seq: i64,
        status: String,
        data: Data,
    },
    Disconnected {
        reason: String,
        will_retry: bool,
    },
}

enum Cmd {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

/// Handle to a running websocket connection task.
///
/// Cloning is cheap; every clone talks to the same connection.
#[derive(Clone)]
pub struct WebSocket {
    cmd: mpsc::UnboundedSender<Cmd>,
    updates: broadcast::Sender<WsUpdate>,
    seq: Arc<AtomicI64>,
    conn_id: Arc<std::sync::Mutex<String>>,
    connected: Arc<AtomicBool>,
}

impl WebSocket {
    /// Spawns the connection task. It reconnects on its own until [`close`] is
    /// called or every receiver is dropped.
    ///
    /// [`close`]: WebSocket::close
    pub fn connect(url: impl Into<String>, token: impl Into<String>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (updates, _) = broadcast::channel(1024);
        let conn_id = Arc::new(std::sync::Mutex::new(String::new()));
        let connected = Arc::new(AtomicBool::new(false));
        let ws = WebSocket {
            cmd: cmd_tx,
            updates: updates.clone(),
            seq: Arc::new(AtomicI64::new(1)),
            conn_id: conn_id.clone(),
            connected: connected.clone(),
        };
        tokio::spawn(run(
            url.into(),
            token.into(),
            cmd_rx,
            updates,
            conn_id,
            connected,
        ));
        ws
    }

    /// Whether a socket is open right now.
    ///
    /// [`connection_id`] keeps its last value while disconnected — it is needed
    /// to attempt a resume — so anything that must not act on a stale identity
    /// should check this first.
    ///
    /// [`connection_id`]: WebSocket::connection_id
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// The current server-assigned connection id, or empty before the first
    /// `hello`.
    ///
    /// Worth having as a getter rather than only an event: [`WsUpdate`] is a
    /// broadcast, so a subscriber that arrives after the socket is already up
    /// never sees the `Connected` that carried it. Anything that needs the id —
    /// Calls uses it as its session identity — should read it here and fall
    /// back to waiting.
    pub fn connection_id(&self) -> String {
        self.conn_id.lock().unwrap().clone()
    }

    /// Subscribes to connection updates. Late subscribers miss earlier events,
    /// so subscribe before you need them.
    pub fn subscribe(&self) -> broadcast::Receiver<WsUpdate> {
        self.updates.subscribe()
    }

    /// Next client-side request sequence. Must be > 0 and monotonic.
    pub fn next_seq(&self) -> i64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// Sends `{"action": …, "seq": …, "data": …}` as a text frame and returns
    /// the sequence it was sent with, so you can match the reply.
    pub fn send_action<T: Serialize>(&self, action: &str, data: T) -> Result<i64> {
        let seq = self.next_seq();
        let req = WsRequest {
            seq,
            action: action.to_string(),
            data,
        };
        let text = serde_json::to_string(&req).map_err(|source| Error::Decode {
            context: "ws request",
            source,
        })?;
        self.cmd
            .send(Cmd::Text(text))
            .map_err(|_| Error::Closed("websocket task stopped".into()))?;
        Ok(seq)
    }

    /// Sends a pre-encoded **binary** frame.
    ///
    /// Mattermost decodes binary frames as msgpack with the same
    /// `{action, seq, data}` field names. This is the path Calls uses for SDP,
    /// because a zlib-compressed SDP has to travel as msgpack `bin` — sending
    /// it as JSON would base64 it and the server's `[]byte` type assertion
    /// would fail *silently*.
    pub fn send_binary(&self, bytes: Vec<u8>) -> Result<()> {
        self.cmd
            .send(Cmd::Binary(bytes))
            .map_err(|_| Error::Closed("websocket task stopped".into()))
    }

    /// Broadcasts a typing indicator into a channel (or a thread, via
    /// `parent_id`).
    pub fn typing(&self, channel_id: &str, parent_id: &str) -> Result<i64> {
        self.send_action(
            "user_typing",
            serde_json::json!({ "channel_id": channel_id, "parent_id": parent_id }),
        )
    }

    /// Tells the server which channel/team you are looking at, which it uses to
    /// scope presence and some notification decisions.
    pub fn set_presence(&self, channel_id: &str, team_id: &str) -> Result<i64> {
        self.send_action(
            "presence",
            serde_json::json!({ "channel_id": channel_id, "team_id": team_id }),
        )
    }

    pub fn request_statuses(&self, user_ids: &[String]) -> Result<i64> {
        self.send_action(
            "get_statuses_by_ids",
            serde_json::json!({ "user_ids": user_ids }),
        )
    }

    /// Stops the connection task for good.
    pub fn close(&self) {
        let _ = self.cmd.send(Cmd::Close);
    }
}

fn backoff(fail_count: u32) -> Duration {
    let base = if fail_count > MAX_FAILS {
        // Quadratic, like the webapp; mobile uses linear.
        MIN_RETRY
            .saturating_mul(fail_count.saturating_mul(fail_count))
            .min(MAX_RETRY)
    } else {
        MIN_RETRY
    };
    let jitter = Duration::from_millis(rand::random::<u64>() % JITTER_MS);
    base + jitter
}

/// Connection state that must survive a reconnect.
struct Session {
    connection_id: String,
    /// The **next** sequence we expect from the server.
    server_sequence: i64,
    last_close_code: Option<u16>,
}

async fn run(
    url: String,
    token: String,
    mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
    updates: broadcast::Sender<WsUpdate>,
    conn_id: Arc<std::sync::Mutex<String>>,
    connected: Arc<AtomicBool>,
) {
    let mut session = Session {
        connection_id: String::new(),
        server_sequence: 0,
        last_close_code: None,
    };
    let mut fail_count: u32 = 0;

    loop {
        let outcome = connect_once(
            &url,
            &token,
            &mut session,
            &mut cmd_rx,
            &updates,
            &conn_id,
            &connected,
        )
        .await;
        connected.store(false, Ordering::Relaxed);
        match outcome {
            Loop::Stop => {
                let _ = updates.send(WsUpdate::Disconnected {
                    reason: "closed by client".into(),
                    will_retry: false,
                });
                return;
            }
            Loop::Retry(reason) => {
                fail_count += 1;
                let _ = updates.send(WsUpdate::Disconnected {
                    reason,
                    will_retry: true,
                });
                tokio::time::sleep(backoff(fail_count)).await;
            }
            Loop::Reconnect => {
                // A clean, intentional reconnect (sequence gap / ping timeout):
                // no penalty, reconnect immediately.
                fail_count = 0;
            }
        }
    }
}

enum Loop {
    /// The client asked us to stop.
    Stop,
    /// Failed or dropped — back off before retrying.
    Retry(String),
    /// Deliberate reconnect, no backoff.
    Reconnect,
}

async fn connect_once(
    base_url: &str,
    token: &str,
    session: &mut Session,
    cmd_rx: &mut mpsc::UnboundedReceiver<Cmd>,
    updates: &broadcast::Sender<WsUpdate>,
    conn_id: &Arc<std::sync::Mutex<String>>,
    connected: &Arc<AtomicBool>,
) -> Loop {
    // A compliant client sends connection_id and sequence_number *together* —
    // the server rejects the upgrade outright if only one is present.
    let mut url = format!(
        "{base_url}?connection_id={}&sequence_number={}",
        session.connection_id, session.server_sequence
    );
    if let Some(code) = session.last_close_code.take() {
        url.push_str(&format!("&disconnect_err_code={code}"));
    }

    let mut request = match url.into_client_request() {
        Ok(r) => r,
        Err(e) => return Loop::Retry(format!("bad websocket url: {e}")),
    };
    match HeaderValue::from_str(&format!("Bearer {token}")) {
        Ok(v) => {
            request.headers_mut().insert(AUTHORIZATION, v);
        }
        Err(e) => return Loop::Retry(format!("bad token: {e}")),
    }

    let connect = tokio_tungstenite::connect_async_tls_with_config(
        request,
        None,
        false,
        crate::tls::ws_connector(),
    );
    let (stream, _resp) = match connect.await {
        Ok(pair) => pair,
        Err(e) => return Loop::Retry(format!("connect failed: {e}")),
    };
    let (mut sink, mut source) = stream.split();
    connected.store(true, Ordering::Relaxed);

    let had_connection = !session.connection_id.is_empty();
    // Assume a successful resume until `hello` tells us otherwise: a resumed
    // stream sends no `hello` at all, which is exactly how we learn it worked.
    // If one does arrive, the handler below announces the new identity.
    if had_connection {
        let _ = updates.send(WsUpdate::Connected {
            connection_id: session.connection_id.clone(),
            resumed: true,
        });
    }

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping.tick().await; // the first tick completes immediately
    let mut ping_outstanding = false;
    let mut ping_seq: i64 = 0;

    loop {
        tokio::select! {
            biased;

            cmd = cmd_rx.recv() => match cmd {
                None | Some(Cmd::Close) => {
                    let _ = sink.close().await;
                    return Loop::Stop;
                }
                Some(Cmd::Text(t)) => {
                    if let Err(e) = sink.send(Message::Text(t.into())).await {
                        return Loop::Retry(format!("send failed: {e}"));
                    }
                }
                Some(Cmd::Binary(b)) => {
                    if let Err(e) = sink.send(Message::Binary(b.into())).await {
                        return Loop::Retry(format!("send failed: {e}"));
                    }
                }
            },

            _ = ping.tick() => {
                if ping_outstanding {
                    // The previous ping was never answered: the socket is
                    // half-open. Real close events can lag by minutes.
                    session.last_close_code = Some(CLOSE_PING_TIMEOUT);
                    let _ = sink.close().await;
                    return Loop::Reconnect;
                }
                ping_seq += 1;
                let payload = serde_json::json!({ "action": "ping", "seq": ping_seq });
                if let Err(e) = sink.send(Message::Text(payload.to_string().into())).await {
                    return Loop::Retry(format!("ping failed: {e}"));
                }
                ping_outstanding = true;
            },

            msg = source.next() => {
                let msg = match msg {
                    None => return Loop::Retry("stream ended".into()),
                    Some(Err(e)) => return Loop::Retry(format!("read failed: {e}")),
                    Some(Ok(m)) => m,
                };

                let text = match msg {
                    Message::Text(t) => t.to_string(),
                    Message::Binary(b) => match String::from_utf8(b.to_vec()) {
                        Ok(t) => t,
                        // The server only ever sends JSON text; anything else
                        // is not ours to interpret.
                        Err(_) => continue,
                    },
                    Message::Close(frame) => {
                        let reason = frame
                            .map(|f| format!("server closed: {} {}", f.code, f.reason))
                            .unwrap_or_else(|| "server closed".into());
                        return Loop::Retry(reason);
                    }
                    // tungstenite answers protocol pings for us.
                    Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                };

                let frame: WsFrame = match serde_json::from_str(&text) {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::warn!(error = %e, "undecodable websocket frame");
                        continue;
                    }
                };

                // Replies bypass sequence validation entirely.
                if frame.is_reply() {
                    let seq = frame.seq_reply.unwrap_or_default();
                    if seq == ping_seq {
                        ping_outstanding = false;
                    }
                    let _ = updates.send(WsUpdate::Reply {
                        seq,
                        status: frame.status.clone().unwrap_or_default(),
                        data: frame.data.clone(),
                    });
                    continue;
                }

                if frame.event == "hello" {
                    let new_id = frame
                        .data
                        .get("connection_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    // A `hello` on a connection we expected to resume means the
                    // server could not replay: our buffer is gone.
                    let lost = had_connection
                        && !session.connection_id.is_empty()
                        && session.connection_id != new_id;
                    if lost {
                        session.server_sequence = 0;
                        let _ = updates.send(WsUpdate::MissedMessages);
                    }
                    session.connection_id = new_id.clone();
                    *conn_id.lock().unwrap() = new_id.clone();

                    // Always announce a `hello`, even after an optimistic
                    // "resumed" announcement: it carries a *new* identity, and
                    // anything keyed on the connection id (Calls sessions, for
                    // one) has to learn about the change to recover.
                    let _ = updates.send(WsUpdate::Connected {
                        connection_id: new_id,
                        resumed: false,
                    });
                }

                if frame.seq != session.server_sequence {
                    tracing::warn!(
                        expected = session.server_sequence,
                        actual = frame.seq,
                        "websocket sequence gap; reconnecting"
                    );
                    session.last_close_code = Some(CLOSE_SEQUENCE_MISMATCH);
                    let _ = sink.close().await;
                    return Loop::Reconnect;
                }
                session.server_sequence = frame.seq + 1;

                let _ = updates.send(WsUpdate::Event(Event::from_frame(&frame)));
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_flat_then_quadratic_and_capped() {
        // First failures: ~3s plus jitter.
        for n in 1..=MAX_FAILS {
            let d = backoff(n);
            assert!(d >= MIN_RETRY, "n={n} d={d:?}");
            assert!(d < MIN_RETRY + Duration::from_millis(JITTER_MS));
        }
        // Then it grows.
        assert!(backoff(MAX_FAILS + 1) > MIN_RETRY + Duration::from_millis(JITTER_MS));
        // And it is capped.
        assert!(backoff(10_000) <= MAX_RETRY + Duration::from_millis(JITTER_MS));
    }
}
