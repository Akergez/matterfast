# Mattermost protocol notes

What follows was derived by reading source, not documentation:
`mattermost/mattermost` (server 11.11.0, `server/public/model/*.go` and
`webapp/platform/client/src/`), `mattermost-plugin-calls`, `rtcd`, and
`calls-common`. Where the published API reference and the code disagree, the
code is what the server does.

The single most useful discovery: **`rtcd/client/` is a complete headless Go
client of the Calls protocol.** It is the normative reference for anything a
non-browser client needs to do, and it is far more informative than the webapp.

---

## 1. Authentication

`POST /api/v4/users/login`. The request body is decoded into a
`map[string]string` server-side, so every value must be a JSON *string* —
booleans included (`"ldap_only": "true"`). The MFA field is `token`, not
`mfa_token`.

The session token comes back in the **`Token` response header**, not the body.

### The cookie beats the header

`app.ParseAuthTokenFromRequest` checks, in order:

1. the `MMAUTHTOKEN` **cookie**
2. `Authorization: Bearer …`
3. `Authorization: Token …`
4. `?access_token=`

If a cookie jar is active *and* you set a bearer token, the cookie wins — and
cookie authentication is the only path where CSRF is enforced
(`checkCSRFToken` requires `tokenLocation == TokenLocationCookie`). A native
client should therefore run with **no cookie store at all** and authenticate by
header, at which point CSRF never applies. `mattermost-api` does this by not
enabling reqwest's `cookies` feature.

---

## 2. WebSocket

`GET /api/v4/websocket`, authenticated either by `Authorization: Bearer` on the
upgrade request or by an in-band `{"action":"authentication_challenge",…}`
frame.

**Prefer the header.** Binary frames are rejected on a connection that has not
yet authenticated (`web_conn.go`, MM-68222), and Calls sends SDP as a binary
msgpack frame.

### Envelopes

```jsonc
// client → server (text = JSON, binary = msgpack, same field names)
{"seq": 1, "action": "user_typing", "data": {"channel_id": "…"}}

// server → client, reply
{"status": "OK", "seq_reply": 1}

// server → client, event
{"event": "posted", "data": {…}, "broadcast": {…}, "seq": 42}
```

A frame with a non-zero `seq_reply` is a reply and **bypasses sequence
validation**; a frame with a non-empty `event` is an event.

Max frame size is **8 KB** (`model.SocketMaxMessageSizeKb`). This is the reason
Calls compresses SDP.

### Almost every payload is double-encoded

`posted.post`, `post_edited.post`, `reaction_added.reaction`,
`preferences_changed.preferences`, `channel_updated.channel`,
`channel_member_updated.channelMember` and most others arrive as **JSON-encoded
strings**, not nested objects. The exceptions — `user_updated.user`,
`config_changed`, `license_changed` — are real objects.

Casing is inconsistent, too: `channelMember`, `updatedCategories`,
`scheduledPost` and `otherFile` are camelCase while everything around them is
snake_case. Calls is worse: `user_joined` carries `user_id` while `user_muted`,
`user_voice_on`, `user_screen_on` and `user_raise_hand` carry `userID`.

Some events put the thing they are about only in the **broadcast envelope**.
`typing` is the classic: the channel id is in `broadcast.channel_id` and nowhere
in `data`.

### Reliable reconnect

Reconnect with `?connection_id=<id>&sequence_number=<next expected>`. Both must
be present together — the server closes the connection outright if only one is.
The server keeps a **128-event dead queue** per connection.

Three outcomes:

1. **Resumed.** The server replays what you missed. You get *no* `hello`, and
   you should do **no REST resync at all**.
2. **Failed resume.** You get a fresh `hello` with a *different*
   `connection_id`. That difference is the only signal, and it means the full
   gap-fill is required.
3. **Mid-stream gap** (`seq != expected`). Do not try to patch it in place:
   close with code 4001 and reconnect.

Also run a ~30 s application-level ping. A real close event can arrive minutes
after a socket is actually dead, and until it does the UI will cheerfully claim
to be connected.

### Post feeds and gaps

`GET /channels/{id}/posts?since=` is the only call that reports **edits and
deletions**, is capped at 1000 posts, and is explicitly **not guaranteed
consecutive**. Posts it returns that are older than your newest block are edits,
not new messages — treating them as new produces a duplicated, misordered feed.

`per_page` is clamped at 200 rather than rejected, so a short page never means
"end of list".

Both official clients model a channel as *several* ordered blocks with gaps
between them (`postsInChannel` with `recent`/`oldest` flags in the webapp,
`PostsInChannel(earliest, latest)` in mobile), render exactly one block at a
time, and merge blocks when they overlap. `recent` means `next_post_id == ""`;
`oldest` means `prev_post_id == ""`. `before`/`after` responses deliberately
include the anchor post so blocks overlap and can merge.

### Unread arithmetic

```
crt ? (messages = channel.total_msg_count_root - member.msg_count_root,
       mentions = member.mention_count_root)
    : (messages = channel.total_msg_count      - member.msg_count,
       mentions = member.mention_count)

show_unread = mentions > 0 || (!muted && messages > 0)
muted       = member.notify_props.mark_unread == "mention"
```

A muted channel still shows mentions; it just does not show message unreads.
The mention decision on a live post comes from `data.mentions` on the `posted`
frame — do not re-parse the message text.

### Threads

A thread is identified by its **root post id**, everywhere. A reply carries
`root_id`; look threads up by `root_id` when it is set and by the post's own id
when it is not.

Under collapsed reply threads (CRT) a reply must **not** enter the channel feed
— both webapp reducers return early for exactly this case, and the server's
feed endpoints filter them out when `collapsedThreads=true`. The only route into
a thread is then the root's `reply_count`, so that counter has to stay live:
increment it locally when a reply arrives over the websocket, or the thread
becomes unreachable until the next full fetch.

`GET /posts/{id}/thread` returns the root and its replies in one `PostList`,
but **`order` is not sorted** — unlike every other feed. The server only adds an
`ORDER BY` when the request names a `direction`, which the official clients do
not send, so what comes back is the requested post followed by replies in
whatever order the query plan produced (`SqlPostStore.Get`). Sort by `create_at`
yourself. Sorting alone is not enough either: two posts can share a millisecond
on a busy server, and then the conversation opens with a reply — so also pull
the root out and put it first.

The CRT inbox is `GET /users/me/teams/{team}/threads`, with `unread=true` for
just the ones needing attention. The official clients re-request the unread set
on *every* sync, because read-state changes do not bump `last_reply_at`.

### Optimistic sends

`pending_post_id` is echoed back on both the REST response and the `posted`
event, and is how a client retires the copy it drew before the round trip
finished. Do not rely on it alone: the response and the event race, and only one
of them is guaranteed to reach you first. Track the pending id locally and
remove that post explicitly when either arrives, or a sent message can appear
twice.

### Recent mentions

There is no mentions endpoint. Mattermost's "Recent Mentions" is a saved search:
`POST /teams/{team}/posts/search` for the user's mention keys, of which
`@username` is the one every account has.

---

## 3. Calls

### Topology

A third-party client **never talks to the SFU's signalling directly** — it has
no credentials for it. Everything goes over Mattermost's own websocket as
plugin-namespaced actions (`custom_com.mattermost.calls_*`). The plugin demuxes
them to an embedded SFU or an external `rtcd`. Media then flows directly between
client and SFU over DTLS/SRTP.

```
client ──HTTPS──► /plugins/com.mattermost.calls/…      discovery
       ──WSS───► /api/v4/websocket                     signalling
                       │
                 Calls plugin
                       ▼
                  rtcd SFU (pion)
       ◄────────DTLS/SRTP/ICE────────►                 media
```

**Your session id is the websocket `connection_id`** from `hello`. It is the
identity used in every Calls event and embedded in every relayed track id.

### Discovery

Base path is `{SiteURL}/plugins/com.mattermost.calls` — *not* under `/api/v4`.

- `GET /version` → decides whether data-channel signalling locking is supported
  (plugin ≥ 1.7.0, and rtcd ≥ 1.1.0 when an external rtcd is in use)
- `GET /config` → ICE servers and feature flags. The keys are **Go field names
  verbatim** (`ICEServersConfigs`, `AllowScreenSharing`, `EnableDCSignaling`)
  because the plugin's configuration struct carries no `json` tags. The nested
  ICE server objects, from a different package, *do* use lowercase tags.
- `GET /turn-credentials` → only when `NeedsTURNCredentials`; short-lived
  HMAC credentials to append to the ICE list

There is no `sfu_url`, no `ws_url` and no `need_upgrade` anywhere in the current
code. If a guide mentions them, it predates the version you are running.

### Five things that fail silently

**1. SDP must be a binary msgpack frame carrying zlib-compressed JSON.**

```
SessionDescription → JSON → zlib (RFC 1950, not raw deflate)
                   → msgpack `bin` → {"data": <bin>} → msgpack map → WS binary frame
```

The plugin does `req.Data["data"].([]byte)`. Send it as JSON and the value
arrives as a base64 *string*, the type assertion fails, and your offer vanishes
with no error returned to you. In Rust this also means `rmp_serde::to_vec_named`
(the server decodes into `map[string]any`, so the envelope must be a msgpack
map) and `serde_bytes` on the payload field (without it you emit an *array*).

ICE candidates, by contrast, are a plain JSON **string** and are not
compressed. And the `signal` events coming *back* are uncompressed JSON too —
the compression is one-directional.

**2. Renegotiation must hold the signalling lock.**

Both peers may renegotiate. The SFU is the *impolite* peer: if an offer arrives
while it is making one of its own, it drops yours — silently. The lock is
arbitrated over the `calls-dc` data channel: send `Lock`, wait for
`Lock`+`true`, retry every 100 ms, 5 s deadline, `Unlock` after the answer.

The lock is gated on the **server version**, not on `EnableDCSignaling` — which
defaults to *off*. Tie the two together and you disable the lock on every stock
server, which is precisely where you need it, because the SFU takes the lock for
its own renegotiations either way.

The **first** negotiation is exempt, and must be, because the SFU is blocking on
your first offer with a 10 s deadline while holding nothing. Take the lock there
and you deadlock against the server.

**3. Audio level must be negotiated *and populated*.**

Voice activity is computed *server-side* from the
`urn:ietf:params:rtp-hdrext:ssrc-audio-level` RTP header extension. Omit it and
you are perfectly audible but never light up as "speaking" for anyone. Note the
SFU's detector is a **variance** detector over a 50-sample window, not a
threshold — a constant level, however loud, never registers as speech, so the
value has to be each packet's real measurement.

**4. ICE candidates arrive before the answer.**

The SFU starts gathering inside `SetLocalDescription`, which runs before it
sends you the answer, so its candidates routinely arrive first. Every WebRTC
stack refuses `addIceCandidate` until a remote description exists, so they must
be queued and flushed after `setRemoteDescription`. Both reference clients do
this; skipping it costs you candidate pairs, and sometimes the whole call.

**5. `reconnect` is required on every websocket reopen.**

Not just when the resume failed. As far as plugins are concerned the server has
no notion of a resumed connection: `OnWebSocketDisconnect` fires whenever a
socket's pump exits, and the Calls plugin then starts a **10 second** timer that
closes the RTC session. `reconnect` — `{channelID, originalConnID, prevConnID}`
— is the only thing that stops it. Until it arrives, every message you send is
dropped because the plugin can no longer find your session. Sending it twice is
harmless; the plugin guards with a compare-and-swap.

### Data-channel framing

The `calls-dc` messages are not msgpack containers. They are a single type byte
followed immediately by an optional msgpack payload, concatenated:

```
[type: u8][payload: msgpack]?
```

Every type value is ≤ 9, so the type byte is a msgpack positive fixint — a bare
`Ping` is literally the single byte `0x01`.

| Type | Value | Payload |
|---|---|---|
| Ping / Pong | 1 / 2 | none |
| Sdp | 3 | msgpack `bin` of zlib(JSON) |
| LossRate / RoundTripTime / Jitter | 4 / 5 / 6 | float64 (RTT and jitter in **seconds**) |
| Lock | 7 | none on request, bool on response |
| Unlock | 8 | none |
| MediaMap | 9 | `mid → {type, sender_id}` |

### Track attribution

The SFU renames every track it relays:

```
genTrackID(type, sessionID) = "{type}_{sessionID}_{8 hex}"
```

so `voice_kj3n8x2q1w9e7r5t4y6u8i0o1p_a1b2c3d4`. Splitting that id is the **only**
reliable way to attribute an incoming track — there is no user id anywhere in
the media plane. Session id → user id comes from `user_joined` and `call_state`.

`MediaMap.sender_id` looks like it would do the job and does not: the SFU fills
it with the *receiving* session's id, not the sender's. No shipping client reads
it. Use it as a `mid → type` hint at most.

`screen-audio` contains a hyphen but no underscore, so a `splitn(3, '_')` handles
it correctly — worth a test, because it looks like it would not.

### Codecs

The SFU registers exactly three: **Opus/111, VP8/96, AV1/45**. No H.264, no VP9.
Register the same set and nothing else, or you negotiate something the SFU will
drop. AV1 is used for screen share only when *both* sender and receiver
advertised `av1Support`, and is mutually exclusive with simulcast in practice.

### Screen sharing ordering

The `screen_on` message carries the MediaStream id and **must be sent before the
track is added**. The SFU classifies an incoming video track purely by comparing
its stream id against the previously announced `screenStreamID`, and *drops* any
video track it cannot classify.

### Mute

Three mechanisms coexist. The signalling flag is the authoritative one: `mute`
and `unmute` set `outVoiceTrackEnabled`, and the SFU simply stops forwarding
your RTP. The reference native client adds the Opus track on first unmute and
thereafter only flips the flag; the browser additionally does
`replaceTrack(null)`.

Note the asymmetry in defaults: the SFU sets `outVoiceTrackEnabled = true` when
it first sees your audio track, while the plugin's stored `unmuted` starts
`false`. Send `unmute` explicitly.

### Join sequence

```
A. GET /plugins/com.mattermost.calls/version            → dc locking?
   GET /plugins/com.mattermost.calls/config             → ICE, flags
   GET .../turn-credentials                             → if NeedsTURNCredentials
B. open the websocket, await `hello`                    → connection_id = session id
   send  calls_join {channelID, jobID:"", av1Support, dcSignaling}
   await calls_join ack with data.connID == our session id
     (failure arrives as calls_error: participant limit, permissions,
      unlicensed group call, no call ongoing)
C. build the PeerConnection, register handlers, then create the `calls-dc`
   data channel — creating it is what makes negotiation "needed"
   create offer → set local → send `sdp` (binary, zlib) — no lock on this one
   trickle ICE as `ice` text frames; do not send an end-of-candidates sentinel
   await `signal` answer → set remote
   ICE → DTLS → SCTP → the data channel opens
   the SFU then offers one renegotiation per already-present remote track
D. add the Opus track (taking the lock), renegotiate, send `unmute`
```

Leaving: send `calls_leave` and close. Just dropping the socket leaves the
plugin holding your slot for a 10 s reconnect grace period.

---

## 4. Things worth knowing that bit somebody already

- Every `*_at` field is Unix **milliseconds**, with exactly two exceptions:
  `Status.dnd_end_time` is **seconds**, and `CustomStatus.expires_at` is
  RFC3339.
- `roles` is a **space-separated string**, not an array.
- `FileInfo.CreatorId` serializes as `user_id`.
- `ChannelStats.PinnedPostCount` serializes as `pinnedpost_count` — no
  underscore between "pinned" and "post".
- `ChannelMember.last_viewed_at` and `last_update_at` are **absent entirely**
  when you fetch another user's membership.
- `Preference.value` is always a string, even for booleans and JSON blobs.
- `sidebar_category_updated` fires on *any* preference change, sometimes with no
  payload — treat it as "refetch categories", not as a delta.
- `channel_viewed` no longer exists; read state arrives as
  `multiple_channels_viewed`.
- Deleting a post is a **soft delete** and also deletes its replies; the
  `post_deleted` event a normal user receives has been sanitized, so do not
  trust its message body.
- `edit_at != 0` marks an edit. `update_at` bumps on reactions and pins too, so
  it cannot be used for that.
- `message_source` holds the user's raw text when the server rewrote `message`
  for presentation — use it when populating an edit box.
- Turning collapsed reply threads on or off is a **mode switch**: mobile
  truncates all thread state and forces a full resync, and so should you.
