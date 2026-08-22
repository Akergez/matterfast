# mattermost-adw

A native Mattermost client for GNOME — GTK4 + libadwaita, in Rust.

<img src="/docs/screenshots/thread.png" />

Four crates:

| Crate | What it is |
|---|---|
| `mattermost-api` | Async Mattermost client: REST v4, models, and a reliable WebSocket with replay-aware reconnect. No GTK dependency. |
| `mattermost-calls` | The Mattermost Calls wire protocol and a WebRTC peer (webrtc-rs). No GTK dependency. |
| `mm-adw` | The application: an adaptive libadwaita window. |
| `mm-testserver` | A fake Mattermost, real enough to exercise the client end to end. Dev-only. |

The two library crates are deliberately independent of the UI — they are usable
for a bot, a CLI, or a different front end.

## Status

**Working**

- Sign in (password + MFA), session token handling, `X-Requested-With`, no cookie jar
- Startup sequence in the order the official clients use it (config → teams → channels → categories)
- Adaptive layout: team rail, channel sidebar, conversation, and a right-hand
  panel that holds either a thread or the inbox
- Sidebar categories with the real ordering rules, unread/mention/muted styling, "call in progress" markers
- Message list: author grouping, day separators, edits, attachments, message
  priority, system messages, webhook name overrides
- **Profile pictures**, cached and shared across the message list, sidebar and
  profile card
- **Reactions as emoji**, not `:shortcodes:` — click a chip to toggle your own,
  or pick from the hover menu
- **Threads**: the reply count opens the thread in the right panel, replies go
  to the root's channel, and collapsed reply threads (CRT) are respected — a
  reply never leaks into the channel feed
- **Profile card** on any avatar or name: picture, presence, position, custom
  status, the person's local time, and a button that opens the DM
- **Inbox**: recent mentions and followed threads, with a count on the header
  button
- Sending messages, optimistically, with the server echo retiring the local copy
- Live updates over the websocket, including a correct reliable-reconnect resync

**Not done yet**

- **Audio capture and playback.** `mattermost-calls` will negotiate a call and
  hand you the remote tracks, but nothing feeds Opus into the outgoing track or
  plays the incoming ones. This is the one piece between "signalling works" and
  "you can talk". PipeWire via GStreamer is the intended route.
- Search, file uploads from the UI, desktop notifications, local persistence,
  screen-share capture, multi-server, message editing and deletion.

## Building

Needs Rust 1.85+, GTK 4.12+ and libadwaita 1.5+.

```sh
# Debian/Ubuntu
sudo apt install libgtk-4-dev libadwaita-1-dev libssl-dev pkg-config build-essential

cargo build --release
cargo test --workspace
```

Run it:

```sh
./target/release/mm-adw
```

To look at the layout without a server:

```sh
MM_ADW_DEMO=1 ./target/release/mm-adw
```

## Testing against a server

`mm-testserver` is a fake Mattermost with real state: it hands out real ids,
serves avatars, keeps threads and reactions, and pushes the same websocket
events the real server does — including the double-encoded payloads and the
inconsistent key casing, because reproducing those is the point.

```sh
cargo run -p mm-testserver          # 127.0.0.1:8065, logs every request

MM_ADW_SERVER=http://127.0.0.1:8065 \
MM_ADW_USER=anton MM_ADW_PASSWORD=test \
  cargo run -p mm-adw               # skips the sign-in form
```

A background "colleague" posts every 12 seconds (`MM_BOT_SECONDS` to change it)
so live updates are visible with one client open. The same three environment
variables work against a real server.

> On a minimal system, install `librsvg2-common` too — without the SVG pixbuf
> loader every symbolic icon renders as a broken-image box, which looks like an
> application bug and is not one.

## Using the libraries

```rust
use mattermost_api::{Client, WebSocket, ws::{WsUpdate, Event}};

let client = Client::new("https://mm.example.com")?;
client.login("alice", "hunter2", None).await?;

let ws = WebSocket::connect(client.websocket_url(), client.token().unwrap());
let mut rx = ws.subscribe();

while let Ok(update) = rx.recv().await {
    match update {
        WsUpdate::Event(Event::Posted(p)) => println!("{}: {}", p.sender_name, p.post.message),
        // The server could not replay its buffer — refetch.
        WsUpdate::MissedMessages => { /* full resync */ }
        _ => {}
    }
}
```

Joining a call:

```rust
use mattermost_calls::{config, CallSession, JoinOptions};

let discovery = config::discover(&client).await?;
let call = CallSession::join(
    &client,
    ws,
    JoinOptions { channel_id: channel_id.into(), ..Default::default() },
    &discovery,
).await?;

let track = call.unmute().await?;   // then write Opus samples into `track`
```

## Notes on the protocol

Mattermost's REST API is documented; its WebSocket payloads and the Calls
protocol largely are not. `docs/PROTOCOL.md` records what was found by reading
`mattermost/mattermost`, `mattermost-plugin-calls`, `rtcd` and `calls-common`,
including several behaviours that fail *silently* if you get them wrong.

## Licence

MIT.
