# Mattermost Calls — Wire Protocol Reverse-Engineering Report

Everything below is verified against the local checkouts at `/tmp/mm/rtcd`, `/tmp/mm/mattermost-plugin-calls`, `/tmp/mm/calls-common` (and `/tmp/mm/mattermost` for the MM server WS internals, which turned out to be present too). Anything I could **not** verify is called out explicitly in §8.

---

## 0. Architecture in one paragraph

A third-party native client **never talks to rtcd directly**. It talks to **Mattermost's ordinary `/api/v4/websocket`**, sending plugin-namespaced actions (`custom_com.mattermost.calls_*`). The Calls plugin (server-side Go, in-process with MM) demuxes those into `rtc.Message` structs and forwards them either to an **embedded** SFU (`rtcServer`, the same `service/rtc` package) or to an **external rtcd** over a separate plugin↔rtcd websocket. Media/ICE go straight from your client to the SFU's UDP/TCP ICE ports. `/tmp/mm/rtcd/client/` is a complete, working headless client of exactly this protocol and is the normative reference.

```
Rust client ──HTTPS──► MM REST  /plugins/com.mattermost.calls/...
            ──WSS───► MM  /api/v4/websocket   (custom_com.mattermost.calls_* actions/events)
                             │
                       Calls plugin (Go)
                             │  rtc.Message (msgpack) over plugin↔rtcd WS, or in-process
                             ▼
                      rtcd service/rtc SFU
            ◄────────DTLS/SRTP/ICE (UDP4/TCP4, default :8443)────────►
```

The **session ID** of your client == the **Mattermost WebSocket `connection_id`** received in the first `hello` event. It is the identity used everywhere: `session_id` in events, the `<sessionID>` embedded in relayed track IDs, `screen_sharing_session_id`, etc.

---

## 1. Discovery / config REST

Base path: `{SiteURL}/plugins/com.mattermost.calls`. Router: `/tmp/mm/mattermost-plugin-calls/server/api_router.go:16-198`.

Auth: every route except `/version`, `/metrics`, `/standalone/` requires an authenticated MM user. MM's plugin HTTP layer sets `Mattermost-User-Id` from a normal MM session, so you authenticate the way any MM API client does:

```
Authorization: Bearer <MM personal access token or session token>
```

Rate limit: 1 req/s, burst 10, per user (`api.go:641-657`, `rate.NewLimiter(1, 10)`) → HTTP 429 with body `{"message": "too many requests", "status_code": 429}`.

### 1.1 `GET /version` (no auth)

`server/api.go:37-52`, struct at `server/public/version.go:6-11`:

```go
type VersionInfo struct {
	Version     string `json:"version"`
	Build       string `json:"build"`
	RTCDVersion string `json:"rtcd_version,omitempty"`
	RTCDBuild   string `json:"rtcd_build,omitempty"`
}
```

```json
{"version":"1.9.0","build":"abc1234","rtcd_version":"1.2.0","rtcd_build":"deadbee"}
```

Used to feature-gate DC signaling locking (`calls-common/src/utils.ts:66-83`): plugin ≥ `1.7.0` **and** (rtcd absent **or** rtcd ≥ `1.1.0`). `"master"` or a `dev*` prefix counts as "latest".

### 1.2 `GET /config`

`server/api.go:601-619`. Non-admins get `ClientConfig`; sysadmins get the whole `configuration` struct with `ClientConfig` embedded. **No `json` tags except `sku_short_name`**, so keys are the Go field names verbatim (`server/configuration.go:97-140`, `512-539`):

```json
{
  "ICEServers": ["stun:stun.example.com:3478"],
  "ICEServersConfigs": [
    {"urls":["stun:stun.example.com:3478"]},
    {"urls":["turn:turn.example.com:3478"],"username":"u","credential":"p"}
  ],
  "AllowEnableCalls": true,
  "DefaultEnabled": true,
  "MaxCallParticipants": 8,
  "NeedsTURNCredentials": false,
  "AllowScreenSharing": true,
  "EnableRecordings": false,
  "EnableTranscriptions": false,
  "EnableLiveCaptions": false,
  "MaxRecordingDuration": 60,
  "EnableSimulcast": false,
  "EnableRinging": false,
  "sku_short_name": "professional",
  "HostControlsAllowed": true,
  "EnableAV1": false,
  "GroupCallsAllowed": true,
  "EnableDCSignaling": false,
  "EnableVideo": false
}
```

ICE server element shape is `rtc.ICEServerConfig` (`rtcd/service/rtc/config.go:175-179`) — note **lowercase** json tags here, unlike the enclosing config:

```go
type ICEServerConfig struct {
	URLs       []string `toml:"urls" json:"urls"`
	Username   string   `toml:"username,omitempty" json:"username,omitempty"`
	Credential string   `toml:"credential,omitempty" json:"credential,omitempty"`
}
```

Semantics of the flags you asked about:
- `AllowScreenSharing` — plugin rejects `screen_on` if false (`server/websocket.go:167-170`).
- `EnableAV1` — client may advertise AV1 receive support in the join message. Mutually exclusive with simulcast in practice (`webapp/src/client.ts:450-458`).
- `EnableRinging` — DM/GM ring UX only; **no protocol-level ringing messages**, it gates client behaviour on `call_start` (`websocket_handlers.ts:174-177`) and the `user_dismissed_notification` REST/WS pair.
- `DefaultEnabled` — "TestMode off". Whether calls are on in channels with no explicit `calls_channels` row.
- `GroupCallsAllowed` — license gate; unlicensed servers get `errGroupCallsNotAllowed` ("unlicensed servers only allow calls in DMs", `server/session.go:25`).
- `EnableTranscriptions` / `EnableLiveCaptions` / `EnableRecordings` — job features; surface as `call_job_state` WS events and `caption` events.
- `EnableDCSignaling` — server permits data-channel signaling; the client opts in per-session via the join message.

**There is no `sfu_url`, no `ws_url`, and no `need_upgrade` anywhere in this codebase** (grepped all three repos). See §8.

### 1.3 `GET /env` — sysadmin only, `map[string]string` of env-var config overrides.

### 1.4 `GET /{channel_id}` (26-char id) — deprecated-but-live channel+call state

`server/api.go:55-113`. No call ongoing → `{"channel_id":"...","enabled":true}`. Call ongoing → adds `"call"`:

```json
{
  "channel_id": "…26…",
  "enabled": true,
  "call": {
    "id": "…26…",
    "start_at": 1712345678901,
    "sessions": [
      {"session_id":"…26…","user_id":"…26…","unmuted":false,"raised_hand":0,"video":false}
    ],
    "thread_id": "…", "post_id": "…",
    "screen_sharing_session_id": "",
    "owner_id": "…", "host_id": "…",
    "recording": {"type":"recording","init_at":0,"start_at":0,"end_at":0,"err":""},
    "transcription": {...}, "live_captions": {...},
    "dismissed_notification": {"<userID>": true}
  }
}
```
Structs: `server/state.go:82-114` (`UserStateClient`, `CallStateClient`, `JobStateClient`).

### 1.5 `GET /channels` — array of the above `{channel_id, enabled, call?}` objects for every channel the caller can read (`api.go:158-251`).

### 1.6 `GET /calls/{channel_id}/active` → `{"active": true|false}` (`api.go:117-140`).

### 1.7 `GET /turn-credentials`

`api.go:561-602`. Only when `NeedsTURNCredentials` is true. Returns `rtc.ICEServers` (array of `ICEServerConfig`) with short-lived HMAC-SHA1 credentials: username = `"<unixExpiry>:<mmUsername>"`, credential = base64(HMAC-SHA1(secret, username)) (`rtcd/service/rtc/turn.go:37-84`). Append these to `ICEServersConfigs` from `/config`.

### 1.8 Call actions

| Method | Path | Body | Notes |
|---|---|---|---|
| POST | `/calls/{call_id}/recording/start` | – | `rtcd/client/api.go:275-290` |
| POST | `/calls/{call_id}/recording/stop` | – | |
| POST | `/calls/{channel_id}/dismiss-notification` | – | sets `dismissed_notification[user]`, emits `user_dismissed_notification` to that user only (`api.go:253-296`) |
| POST | `/calls/{channel_id}/decline` | – | DM decline; ends call + emits `call_end` and `user_dismissed_notification` |
| POST | `/calls/{call_id}/host/make` | `{"new_host_id":"…"}` | |
| POST | `/calls/{call_id}/host/mute` | `{"session_id":"…"}` | emits `host_mute` to the target only |
| POST | `/calls/{call_id}/host/screen-off` | `{"session_id":"…"}` | |
| POST | `/calls/{call_id}/host/lower-hand` | `{"session_id":"…"}` | |
| POST | `/calls/{call_id}/host/remove` | `{"session_id":"…"}` | broadcast `host_removed` |
| POST | `/calls/{call_id}/host/mute-others` | – | |
| POST | `/calls/{call_id}/host/end` | – | also `POST /calls/{call_id}/end` for pre-2.18 mobile |
| POST | `/cloud-notify-admins` | – | |
| GET | `/stats` | – | sysadmin |
| POST | `/logs/upload` | JSON, ≤2MB | |

Host-control endpoints are **advisory**: the server does not force-mute you, it sends you a `host_mute` WS event and expects you to mute yourself (`server/host_controls.go:103-107`).

---

## 2. Signaling transports

There are exactly **three** transports in the codebase. Only #1 and #3 are usable by a third-party client.

### 2.1 Main Mattermost WebSocket — the one you use

**URL**: `{ws|wss}://{host}{basename}/api/v4/websocket`

Constructed by `rtcd/client/config.go:50-57`:
```go
if u.Scheme == "http" { u.Scheme = "ws"; u.Path += mmWebSocketAPIPath } else { u.Scheme = "wss"; u.Path += mmWebSocketAPIPath }
```
`mmWebSocketAPIPath = "/api/v4/websocket"` (`rtcd/client/websocket.go:23`).

**Query params** (optional, for lossless reconnect; the Go client does **not** use them, the webapp does):
`?connection_id=<prevConnID>&sequence_number=<lastServerSeq>` — `mattermost/server/channels/api4/websocket.go:19-20, 99-112`. Also honoured: `posted_ack`, `disconnect_err_code`.

**Auth — two options:**

1. **HTTP header (recommended, what the Go client does)**: `Authorization: Bearer <token>` on the upgrade request — `rtcd/service/ws/client.go:65-70`, selected by `AuthType: ws.BearerClientAuthType` in `rtcd/client/websocket.go:337-341`.
2. **In-band challenge (what the webapp does)**: first frame after open —
   ```json
   {"action":"authentication_challenge","seq":1,"data":{"token":"<token>"}}
   ```
   `webapp/src/websocket.ts:66-72`.

⚠️ **Binary frames are rejected on unauthenticated connections** (`mattermost/server/channels/app/platform/web_conn.go:471-475`, MM-68222). Since SDP is sent as a **binary msgpack frame**, you must use the `Authorization` header, or complete `authentication_challenge` before sending SDP.

**Framing — both directions:**

- **Client→server, text frame**: JSON of `model.WebSocketRequest` (`mattermost/server/public/model/websocket_request.go:18-28`):
  ```json
  {"action":"custom_com.mattermost.calls_join","seq":2,"data":{...}}
  ```
- **Client→server, binary frame**: **msgpack** of the same struct (map with keys `action`, `seq`, `data`). Decoder selection: `web_conn.go:477-489` — `TextMessage → json.NewDecoder`, anything else → `msgpack.NewDecoder`.
- **Server→client**: always **JSON text**, envelope `webSocketEventJSON` (`public/model/websocket_message.go:234-239`):
  ```json
  {"event":"custom_com.mattermost.calls_signal",
   "data":{...},
   "broadcast":{"omit_users":null,"user_id":"","channel_id":"","team_id":"","connection_id":"…"},
   "seq":42}
  ```

**Max frame size**: `model.SocketMaxMessageSizeKb = 8 * 1024` = **8 KB** (`public/model/websocket_client.go:21`). This is exactly why SDP is zlib-compressed.

**Action routing**: MM forwards any action prefixed `custom_` **only** to plugins, never to its own router (`web_conn.go:490-493`, `websocketMessagePluginPrefix = "custom_"`). The Calls plugin then strips `wsActionPrefix = "custom_" + manifest.Id + "_"` = `custom_com.mattermost.calls_` (`build/manifest/main.go:30,40`; used at `server/websocket.go:1199-1203`).

**Keepalive**: server pings; webapp additionally sends `{"action":"ping","seq":N}` every 5 s and expects `{"seq_reply":N}` (`webapp/src/websocket.ts:298-309`). The Go client does not — server-level WS ping/pong suffices. Server drops the conn after ~2×ping interval of silence.

**Rate limit on plugin messages**: `rate.NewLimiter(10, 100)` per session (`server/session.go:76`); exceeding it silently drops your messages.

### 2.2 The plugin's own websocket — **does not exist**

There is no separate plugin-hosted WS endpoint. `api_router.go` registers only HTTP handlers. The only WS the plugin participates in is MM's own, via the `WebSocketMessageHasBeenPosted` / `OnWebSocketDisconnect` plugin hooks.

### 2.3 Direct websocket to rtcd — exists, but is **plugin→rtcd only**

`{rtcdURL}/ws`, msgpack binary frames, `ClientMessage{Type string, Data any}` with custom msgpack encoding as a **2-element array** `[type, data]` (`rtcd/service/client_msg.go:29-33`). Types: `join`, `leave`, `rtc`, `hello`, `reconnect`, `close`, `vad`. Auth is a registered `clientID`/`authKey` pair obtained via `POST /register` / `POST /login` on rtcd's HTTP API (`rtcd/service/service.go:117-123`). The plugin embeds credentials in `RTCDServiceURL` userinfo (`server/utils.go:115-126`). **A third-party client has no credentials for this and must not use it.**

---

## 3. Complete signaling message catalog

Client action strings are all prefixed `custom_com.mattermost.calls_`; server event strings likewise. Constants: `rtcd/client/websocket.go:30-66` and `mattermost-plugin-calls/server/client_message.go:15-35` / `server/websocket.go:26-58`.

### 3.1 Client → server

| Action (after prefix) | Frame | `data` payload | Handler |
|---|---|---|---|
| `join` | JSON | `CallJoinMessage` | `server/websocket.go:1236-1284` |
| `reconnect` | JSON | `CallReconnectMessage` | `websocket.go:1285-1308` |
| `leave` | JSON | none | `websocket.go:1309-1325` |
| `call_state` | JSON | `{"channelID":"…"}` | `websocket.go:1326-1338` |
| `sdp` | **binary msgpack** | `{"data": <bin: zlib(JSON SessionDescription)>}` | `websocket.go:1339-1350` |
| `ice` | JSON or msgpack | `{"data": "<JSON string of RTCIceCandidateInit>"}` | `websocket.go:1351-1357` |
| `mute` / `unmute` | JSON | none | `websocket.go:318-377` |
| `screen_on` | JSON | `{"data":"{\"screenStreamID\":\"…\"}"}` | `websocket.go:167-246` |
| `screen_off` | JSON | none | ” |
| `video_on` | JSON | `{"data":"{\"videoStreamID\":\"…\"}"}` | `websocket.go:382-470` |
| `video_off` | JSON | none | ” |
| `raise_hand` / `unraise_hand` | JSON | none | `websocket.go:471-509` |
| `react` | JSON | `{"data":"{\"name\":\"+1\",\"unified\":\"1f44d\",\"skin\":\"\",\"literal\":\"👍\"}"}` | `websocket.go:510-531` |
| `caption` | JSON | bot-only | `websocket.go:1365-1391` |
| `metric` | JSON | `{"metric_name":"client_ice_candidate_pair","data":"<json>"}` | `websocket.go:1392-1404` |
| `ping` | JSON | – | passthrough, ignored by plugin (`websocket.go:1211-1214`) |

`voice_on` / `voice_off` are in the valid-type table (`client_message.go:53-54`) but **`handleClientMsg` has no case for them** (`websocket.go:266-534`) → sending them yields `invalid client message type` server-side. VAD is server-generated; see §5.3.

**`join` payload** (`rtcd/client/types.go:8-13`, plugin reads it at `websocket.go:1237-1272`):
```json
{"channelID":"…26…","jobID":"","av1Support":false,"dcSignaling":false}
```
The plugin additionally accepts `title` (string) and `threadID` (string) — used only when *starting* a call (creates the "call started" post / attaches to a thread). `jobID` is bot-only and **must** be empty for a normal user (`websocket.go:738-740` enforces the inverse for the bot).

**`reconnect` payload** (`rtcd/client/types.go:15-19`):
```json
{"channelID":"…","originalConnID":"…","prevConnID":"…"}
```
All three required or the plugin logs an error and drops it (`websocket.go:1286-1300`).

**`sdp` payload — the exact wrapping**, from `rtcd/client/rtc.go:175-185`:
```go
var sdpData bytes.Buffer
w := zlib.NewWriter(&sdpData)
if err := json.NewEncoder(w).Encode(answer); err != nil { w.Close(); return ... }
w.Close()

return c.SendWS(wsEventSDP, map[string]any{
    "data": sdpData.Bytes(),
}, true)          // <-- true == msgpack BINARY frame
```
So the byte order is: `webrtc.SessionDescription` → `json.Marshal` (`{"type":"offer","sdp":"v=0\r\n..."}`) → **zlib (RFC1950, not raw deflate)** → msgpack `bin` → inside `{"action","seq","data":{"data":<bin>}}` → msgpack → WS binary frame.

The plugin decodes with `req.Data["data"].([]byte)` then `unpackSDPData` (`server/utils.go:99-113`): zlib reader, hard cap `sdpDataMaxSize = 64 * 1024` (`server/utils.go:30-32`) on the *decompressed* size. If you sent SDP as JSON, `[]byte` would arrive as a base64 string and the type assertion fails → message silently dropped. **SDP must be binary.**

Webapp does exactly the same with fflate (`webapp/src/client.ts:559-570`):
```ts
const payload = JSON.stringify(sdp);
ws.send('sdp', { data: zlibSync(strToU8(payload)) }, true);
```

**`ice` payload** — *not* compressed, a JSON **string** inside `data` (`rtcd/client/rtc.go:256-266`):
```go
data, _ := json.Marshal(candidate.ToJSON())
c.SendWS(wsEventICE, map[string]any{"data": string(data)}, true)
```
Webapp uses a text frame for the same thing (`client.ts:572-576`): `ws.send('ice', {data: JSON.stringify(candidate)})`. Both work — the plugin asserts `req.Data["data"].(string)` (`websocket.go:1351-1357`), and msgpack `str` also decodes to a Go `string`. Content is a `RTCIceCandidateInit`:
```json
{"candidate":"candidate:1 1 udp 2130706431 10.0.0.1 8443 typ host","sdpMid":"0","sdpMLineIndex":0,"usernameFragment":"…"}
```
The SFU unmarshals it into `webrtc.ICECandidateInit` and skips empty candidates (`rtcd/service/rtc/session.go:270-299`) — i.e. **do not send an end-of-candidates sentinel**; pion's `OnICECandidate(nil)` case is simply not forwarded (`rtcd/client/rtc.go:249-252`).

### 3.2 Server → client

Every event's `data` may carry `"channelID"` (added for bot broadcasts, `server/websocket.go:130-134`) and `"connID"` for connection-targeted ones. Otherwise use `broadcast.channel_id` / `broadcast.connection_id`. The Go client's filter (`rtcd/client/websocket.go:152-160`):
```go
msgConnID := ev.GetBroadcast().ConnectionId
if msgConnID == "" { msgConnID, _ = ev.GetData()["connID"].(string) }
if msgConnID != "" && msgConnID != c.currentConnID && msgConnID != c.originalConnID { return nil }
```

| Event (after prefix) | `data` | Source |
|---|---|---|
| `join` | `{"connID":"<yourConnID>"}` | `websocket.go:939-941` — **your join ACK** |
| `error` | `{"data":"<message>","connID":"…"}` | `websocket.go:788-791, 1277-1280` |
| `signal` | `{"data":"<JSON SessionDescription or candidate wrapper>","connID":"<sessionID>"}` | `websocket.go:682-685` |
| `call_start` | `{"id","channelID","start_at","thread_id","post_id","owner_id","host_id"}` | `websocket.go:825-833` |
| `call_state` | `{"channel_id":"…","call":"<JSON string of CallStateClient>"}` | `websocket.go:963-966`, `1186-1189` |
| `call_end` | `{}` (+`channelID` for bot) | `host_controls.go:288`, `api.go:352` |
| `user_joined` | `{"user_id","session_id"}` | `websocket.go:943-946` |
| `user_left` | `{"user_id","session_id"}` | `session.go:436-439` |
| `user_muted` / `user_unmuted` | `{"userID","session_id"}` | `websocket.go:370-377` |
| `user_voice_on` / `user_voice_off` | `{"userID","session_id"}` | `websocket.go:662-680` (SFU VAD) |
| `user_screen_on` / `user_screen_off` | `{"userID","session_id"}` | `websocket.go:240-243` |
| `user_video_on` / `user_video_off` | `{"userID","session_id"}` | `websocket.go:463-470` |
| `user_raise_hand` / `user_unraise_hand` | `{"userID","session_id","raised_hand":<ms>}` | `websocket.go:501-509` |
| `user_reacted` | `{"user_id","session_id","emoji":{"name","skin","unified","literal"},"timestamp":<ms>}` | `websocket.go:523-531` |
| `call_host_changed` | `{"hostID","call_id"}` | `host_controls.go:66`, `session.go:210,449` |
| `call_job_state` | `{"callID","jobState":{"type","init_at","start_at","end_at","err"}}` | `websocket.go:949-956` |
| `job_stop` | `{"job_id"}` | consumed at `rtcd/client/websocket.go:235-237` |
| `user_dismissed_notification` | `{"userID","callID"}` | `api.go:286-289` |
| `caption` | `{"channel_id","user_id","session_id","text"}` | `websocket.go:1552-1561` |
| `host_mute` | `{"channel_id","session_id"}` | `host_controls.go:103` |
| `host_screen_off` | `{"channel_id","session_id"}` | `host_controls.go:166` |
| `host_lower_hand` | `{"call_id","channel_id","session_id","host_id"}` | `host_controls.go:199` |
| `host_removed` | `{"call_id","channel_id","session_id","user_id"}` | `host_controls.go:232` |
| `channel_enable_voice` / `channel_disable_voice` | `null` | `api.go:558` |

Plus MM's native `hello` (`{"connection_id":"…","server_version":"…"}`) and `user_removed`.

**`user_connected` / `user_disconnected` do not exist in this version** — they were replaced by `user_joined`/`user_left` (session-scoped, carrying `session_id`). `calls-common/src/types/types.ts:29-35` still declares `UserConnectedData`/`UserDisconnectedData` types but nothing emits or registers them (`webapp/src/index.tsx:233-330` registers only `user_joined`/`user_left`).

**Ringing** has no dedicated message. It is `call_start` + client-side `EnableRinging` (`webapp/src/websocket_handlers.ts:174-177`), cancelled by `user_dismissed_notification` or by the DM `/decline` endpoint.

### 3.3 The `signal` event's inner payload

`data.data` is a **plain, uncompressed JSON string** (`server/websocket.go:682-684`: `"data": string(msg.Data)`). Two shapes, discriminated by `type`:

```json
{"type":"offer","sdp":"v=0\r\n..."}
{"type":"answer","sdp":"v=0\r\n..."}
{"type":"candidate","candidate":{"candidate":"candidate:...","sdpMid":"0","sdpMLineIndex":0,"usernameFragment":"..."}}
```

Note the **asymmetry**: for `candidate` the actual init object is nested one level under `"candidate"`. Built at `rtcd/service/rtc/msg.go:59-68`:
```go
func newICEMessage(s *session, c *webrtc.ICECandidate) (Message, error) {
	data := make(map[string]interface{})
	data["type"] = "candidate"
	data["candidate"] = c.ToJSON()
	js, err := json.Marshal(data)
	...
}
```
Parsed by the Go client at `rtcd/client/rtc.go:59-86` — it only reads `msg["candidate"]["candidate"]` (the string), discarding `sdpMid`/`sdpMLineIndex`, and queues candidates until a remote description exists.

For SDP the SFU marshals `s.rtcConn.LocalDescription()` directly (`rtcd/service/rtc/session.go:376`, `562`), so the outer object *is* the `SessionDescription` — no `"data"` nesting, no zlib **on this direction**.

### 3.4 Data-channel signaling (`dcSignaling`)

Channel label: **`calls-dc`**, created by the **client** (`rtcd/client/rtc.go:426`, `calls-common/src/rtc_peer.ts:89`), default options (ordered, reliable). Browser sets `binaryType = 'arraybuffer'`. The SFU picks it up via `OnDataChannel` (`rtcd/service/rtc/sfu.go:381-383`).

**Wire format** (`rtcd/service/rtc/dc/msg.go:15-90`, mirrored in `calls-common/src/dc_msg.ts:6-49`): *flat* msgpack — a single-byte type followed immediately by an optional payload, **with no enclosing array or map**.

```go
type MessageType uint8
const (
	MessageTypePing          MessageType = iota + 1 // 1  no payload
	MessageTypePong                                 // 2  no payload
	MessageTypeSDP                                  // 3  bin: zlib(JSON SessionDescription)
	MessageTypeLossRate                             // 4  float64
	MessageTypeRoundTripTime                        // 5  float64
	MessageTypeJitter                               // 6  float64
	MessageTypeLock                                 // 7  bool (response only; request has no payload)
	MessageTypeUnlock                               // 8  no payload
	MessageTypeMediaMap                             // 9  MediaMap
)
```
```go
func EncodeMessage(mt MessageType, payload any) ([]byte, error) {
	enc := msgpack.GetEncoder(); defer msgpack.PutEncoder(enc)
	var buf bytes.Buffer; enc.ResetWriter(&buf)
	var err error
	if payload != nil {
		if mt == MessageTypeSDP {
			payload, err = packData(payload.([]byte))   // zlib
			...
		}
		err = enc.EncodeMulti(mt, payload)              // <-- concatenation, no container
	} else {
		err = enc.EncodeUint8(uint8(mt))
	}
	return buf.Bytes(), err
}
```
Because all type values are ≤ 9, the msgpack encoding of the type byte is a **positive fixint**, i.e. literally one byte equal to the enum value. The TS test asserts this: `expect(pingMsg).toEqual(new Uint8Array([DCMessageType.Ping]))` (`calls-common/src/dc_msg.test.ts:13`).

Payloads:
- **SDP (3)**: msgpack `bin` containing `zlib(JSON.stringify({type,sdp}))`. Note the *outer* msgpack is the bin header + data; there is no zlib on the type byte.
- **LossRate/RTT/Jitter (4/5/6)**: msgpack float64. Units — LossRate is a fraction 0..1; RTT is **seconds** (`rtcd/client/rtc.go:469` sends `float64(rtt/1000)` from ms; `rtc_peer.ts:104` computes `(performance.now()-lastPingTS)/1000`); Jitter is seconds.
- **Lock (7)**: request = bare byte `0x07`. Response from SFU = `0x07` + msgpack bool.
- **MediaMap (9)**: msgpack map `mid → {"type": string, "sender_id": string}` (`dc/msg.go:33-38`):
  ```go
  type TrackInfo struct {
      Type     string `msgpack:"type"`      // "voice", "screen", "screen-audio", "video"
      SenderID string `msgpack:"sender_id"` // the session ID of the sender
  }
  type MediaMap map[string]TrackInfo
  ```

**Ping/pong**: client sends `Ping` every 1 s (`rtcd/client/rtc.go:435-450`; `rtc_peer.ts:136-144`), SFU replies `Pong` (`rtcd/service/rtc/dc.go:82-90`). The SFU also still answers a *literal text* `"ping"` with `"pong"` for legacy mobile (`dc.go:57-64`).

**The signaling lock protocol** (this is the subtle bit). The SFU owns a single `dc.Lock` per session (`rtcd/service/rtc/dc/lock.go`), and both sides must hold it before starting a renegotiation:

- Client wants to renegotiate → sends `Lock` (no payload) → SFU `TryLock()` → replies `Lock` + `true|false`. On `false`, retry after 100 ms (Go: `rtcd/client/rtc.go:557-586`) or 50 ms (TS: `rtc_peer.ts:183-255`, `signalingLockCheckIntervalMs = 50`). Timeout 5 s.
- After the client receives the SFU's **answer**, it sends `Unlock` (`rtcd/client/rtc.go:125-132`, `541-555`).
- **Exception**: the very first negotiation is not locked. `dcNegotiationStarted`/`dcNegotiated` gate this — first `OnNegotiationNeeded` skips the lock; the first answer sets `dcNegotiated = true` instead of unlocking (`rtc.go:406-422` and `125-132`).
- SFU-initiated renegotiation (adding/removing a relayed track) takes the same lock server-side: `us.signalingLock.Lock(signalingLockTimeout /*5s*/)` before `addTrack`, `Unlock()` after the answer (`rtcd/service/rtc/sfu.go:910-944`).

If `dcLocking` is off (old server), skip all `Lock`/`Unlock` traffic — see the version gate in §1.1.

**SDP may travel on either transport at any time.** `handleOffer` prefers the DC if `EnableDCSignaling && dc != nil && dc.ReadyState()==Open`, else falls back to the WS (`rtcd/client/rtc.go:156-186`). The SFU replies on whichever channel the offer arrived on: `handleIncomingSDP(us, answerCh, data)` is called with `s.receiveCh` (→ WS `signal` event) from `server.go:254` and with `us.dcSDPCh` (→ DC) from `dc.go:92`. SFU-initiated offers go to `sdpCh := s.receiveCh; if us.dcSignaling() { sdpCh = us.dcSDPCh }` (`sfu.go:905-908`).

---

## 4. WebRTC specifics

### 4.1 Who offers

- **Initial negotiation: the client offers.** The SFU's `handleDCNegotiation` *blocks* on `us.sdpOfferInCh` with a 10 s `signalingTimeout` and kills the session if none arrives (`rtcd/service/rtc/session.go:146-189`, `server.go:23`). The client's offer is produced by pion's `OnNegotiationNeeded`, which fires because the client creates the `calls-dc` data channel (`rtcd/client/rtc.go:424-430`, with the comment *"DC creation must happen after OnNegotiationNeeded has been registered to avoid races"*).
- **Subsequent renegotiation is bidirectional.** The SFU offers whenever it adds/removes a relayed track (`session.go:365-391 sendOffer`, called from `addTrack`/`removeTrack`); the client offers when *it* adds/removes a local track (mic, screen, camera). Hence the lock.
- **Glare handling**: the SFU is *impolite* — `hasSignalingConflict()` returns true if `makingOffer || SignalingState() != Stable` and it **silently drops your offer** (`session.go:543-583`). The client is *polite* and proceeds anyway (`rtc_peer.ts:330-332`). Practical consequence for Rust: always take the lock before offering; a dropped offer produces no error, just a hang.

### 4.2 Config / transceivers / m-lines

Client side (`rtcd/client/rtc.go:188-235`):
```go
cfg := webrtc.Configuration{
	ICEServers:   []webrtc.ICEServer{}, // TODO: consider loading ICE servers from config
	SDPSemantics: webrtc.SDPSemanticsUnifiedPlan,
}
...
s := webrtc.SettingEngine{}
s.EnableSCTPZeroChecksum(true)
s.SetNetworkTypes([]webrtc.NetworkType{webrtc.NetworkTypeUDP4, webrtc.NetworkTypeTCP4})
```
SFU side (`rtcd/service/rtc/sfu.go:82-101, 254-257`): Unified Plan, SCTP zero-checksum enabled, mDNS **disabled**, loopback candidates included, UDP4+TCP4 (+IPv6 if configured).

**M-line layout**: nothing is pre-allocated. The first offer contains only the `application` m-section for `calls-dc`. Every subsequent m-line is appended by whoever adds a track:
- Your mic → you `AddTrack` → one `sendrecv`/`sendonly` audio m-line.
- Your screen → you `AddTransceiverFromTrack(..., Direction: Sendonly)` (`rtcd/client/api.go:104`); the browser uses `direction: 'sendrecv'` (`rtc_peer.ts:433-437`).
- Each remote participant's relayed track → the SFU `AddTrack`s it and offers you a new **recvonly** m-line. One m-line per remote *track*, not per participant: a participant sharing screen with audio while unmuted contributes 3 (`voice`, `screen`, `screen-audio`).

Screen sharing is **exclusive per call** — `call.setScreenSession` refuses a second sharer (`rtcd/service/rtc/call.go:76-84`) and the plugin rejects `screen_on` if `ScreenSharingSessionID != ""` (`server/websocket.go:181-191`).

### 4.3 Codecs (SFU's `MediaEngine` — this is the negotiation ceiling)

`rtcd/service/rtc/sfu.go:31-63, 129-156`:
```go
rtpAudioCodec = webrtc.RTPCodecCapability{
	MimeType:     "audio/opus",
	ClockRate:    48000,
	Channels:     2,
	SDPFmtpLine:  "minptime=10;useinbandfec=1",
	RTCPFeedback: nil,
}
// registered with PayloadType 111
rtpVideoCodecs = map[string]webrtc.RTPCodecParameters{
	webrtc.MimeTypeVP8: { ... ClockRate: 90000, SDPFmtpLine: "", RTCPFeedback: videoRTCPFeedback, PayloadType: 96 },
	webrtc.MimeTypeAV1: { ... ClockRate: 90000, SDPFmtpLine: "", RTCPFeedback: videoRTCPFeedback, PayloadType: 45 },
}
videoRTCPFeedback = []webrtc.RTCPFeedback{
	{Type: "goog-remb"}, {Type: "ccm", Parameter: "fir"}, {Type: "nack"}, {Type: "nack", Parameter: "pli"},
}
```
**Only Opus (111), VP8 (96) and AV1 (45). No H.264, no VP9, no G.711/telephone-event.** Test assertions confirm the payload types on the wire: `require.Equal(t, webrtc.PayloadType(0x6f), track.PayloadType())` (=111, opus) and `0x60` (=96, VP8) at `rtcd/client/rtc_test.go:464-470`.

**Header extensions registered by the SFU** (`sfu.go:143-153`):
- audio: `urn:ietf:params:rtp-hdrext:ssrc-audio-level` — **required for VAD / `user_voice_on`** (`sfu.go:484-500`).
- video: `sdes:mid`, `sdes:rtp-stream-id`, `sdes:repaired-rtp-stream-id` (`sfu.go:64-68`) — required for simulcast. The Go client registers the same three (`rtcd/client/rtc.go:36-40, 217-221`).
- TWCC is added by `ConfigureTWCCSender` + `ConfigureTWCCHeaderExtensionSender` (`sfu.go:190-217`), together with a GCC send-side BWE (min = 0.5×500 kbps, max = 1.5×2.5 Mbps) used to drive simulcast layer selection.

Interceptors on the SFU: NACK generator + responder (buffer 256, configurable 32–8192, must be a power of 2), RTCP reports, TWCC, congestion control.

### 4.4 Track identification — the core mapping

**The SFU renames every relayed track.** `rtcd/service/rtc/utils.go:18-58`:
```go
type trackType string
const (
	trackTypeVoice       trackType = "voice"
	trackTypeScreen      trackType = "screen"
	trackTypeScreenAudio trackType = "screen-audio"
	trackTypeVideo       trackType = "video"
)

func genTrackID(tt trackType, baseID string) string {
	return string(tt) + "_" + baseID + "_" + random.NewID()[0:8]
}
```
and creates the outbound track with `baseID = us.cfg.SessionID`, `streamID = us.cfg.SessionID`:
```go
// audio — sfu.go:439
outAudioTrack, err := webrtc.NewTrackLocalStaticRTP(rtpAudioCodec, genTrackID(trackType, us.cfg.SessionID), us.cfg.SessionID)

// video/screen — sfu.go:570-571
outTrack, err := webrtc.NewTrackLocalStaticRTP(params.RTPCodecCapability,
	genTrackID(outTrackType, us.cfg.SessionID), us.cfg.SessionID, webrtc.WithRTPStreamID(remoteTrack.RID()))
```

So the SDP you receive carries `a=msid:<senderSessionID> <type>_<senderSessionID>_<8 hex chars>`, e.g.

```
a=msid:kj3n8x2q1w9e7r5t4y6u8i0o1p voice_kj3n8x2q1w9e7r5t4y6u8i0o1p_a1b2c3d4
```

**The client-side parse** (`rtcd/client/utils.go:11-20`):
```go
// ParseTrackID returns the track type and session ID for the given track ID.
func ParseTrackID(trackID string) (string, string, error) {
	fields := strings.Split(trackID, "_")
	if len(fields) < 3 {
		return "", "", fmt.Errorf("invalid trackID %q", trackID)
	}
	return fields[0], fields[1], nil
}
```
⚠️ `"screen-audio"` splits to `["screen-audio", sessionID, rand]` — the hyphen is fine, but note the *server's* `getTrackType`/`isValidTrackID` require **exactly 3** fields while the client accepts `>= 3`.

**And the consumer** (`rtcd/client/rtc.go:294-351`):
```go
pc.OnTrack(func(track *webrtc.TrackRemote, receiver *webrtc.RTPReceiver) {
	trackType, sessionID, err := ParseTrackID(track.ID())
	if err != nil { ...; receiver.Stop(); return }
	if trackType != TrackTypeVoice && trackType != TrackTypeScreen && trackType != TrackTypeVideo {
		c.log.Debug("ignoring unsupported track type", slog.Any("trackType", trackType))
		receiver.Stop(); return
	}
	if trackType == TrackTypeScreen {
		// request a keyframe immediately
		pc.WriteRTCP([]rtcp.Packet{&rtcp.PictureLossIndication{MediaSSRC: uint32(track.SSRC())}})
	}
	c.mut.Lock()
	c.receivers[sessionID] = append(c.receivers[sessionID], receiver)
	c.mut.Unlock()
	...
})
```
Test proof that `sessionID` really is the other participant's MM connection id (`rtcd/client/rtc_test.go:458-461`):
```go
trackType, sessionID, err := ParseTrackID(track.ID())
require.NoError(t, err)
require.Equal(t, th.userClient.originalConnID, sessionID)
```

**session ID → user ID** comes from the WS layer: `user_joined` `{user_id, session_id}` and `call_state.call.sessions[]` `{session_id, user_id, …}`. There is no user ID in the media plane at all.

**The MediaMap alternative** (browsers). `calls-common/src/rtc_peer.ts:306-308`:
```ts
private onTrack(ev: RTCTrackEvent) {
    this.emit('stream', new MediaStream([ev.track]), this.mediaMap[ev.transceiver.mid!]);
}
```
The SFU sends the map right before every offer (`rtcd/service/rtc/session.go:637-668`):
```go
mediaMap[trx.Mid()] = dc.TrackInfo{
	Type:     string(trackType),
	SenderID: s.cfg.SessionID,
}
```
⚠️ **Bug / caveat**: `s` there is the *receiving* session, so `sender_id` is the receiver's own session id, not the originating sender's. Nothing consumes it today (grep for `sender_id` across `calls-common` and `webapp` finds only the type definition). **Use `ParseTrackID(track.id)` — do not trust `MediaMap.sender_id`.** Use MediaMap only as a `mid → type` hint if you want it.

### 4.5 Simulcast

Screen only, and only when `EnableSimulcast` is on. Levels are `"h"` and `"l"` (`rtcd/service/rtc/simulcast.go:19-35`), default = `"l"`:
```go
var simulcastRates = map[string]int{ SimulcastLevelHigh: 2_500_000, SimulcastLevelLow: 500_000 }
```
Browser encodings (`calls-common/src/rtc_peer.ts:23-29`):
```ts
const DefaultSimulcastScreenEncodings = [
    {rid: 'l', maxBitrate: 500 * 1000,  maxFramerate: 5,  scaleResolutionDownBy: 1.0},
    {rid: 'h', maxBitrate: 2500 * 1000, maxFramerate: 20, scaleResolutionDownBy: 1.0},
];
const FallbackScreenEncodings = [
    {maxBitrate: 1000 * 1000, maxFramerate: 10, scaleResolutionDownBy: 1.0},
];
```
Native client (`rtcd/client/api.go:104-114`): pass two tracks with different RIDs; `trx.Sender().AddEncoding(tracks[1])`. Max 2 encodings.

The SFU picks a layer per *receiver* from its GCC estimate and swaps the relayed track (remove+add, two renegotiations) when the level changes, with exponential backoff starting at 10 s (`simulcast.go:57-259`). **AV1 and simulcast are mutually exclusive** in practice (the webapp warns if both are set, `client.ts:456-458`).

**AV1 selection** (`rtcd/service/rtc/sfu.go:632-642` and `822-828`): the AV1 relay is used only when *both* the sender and the receiver set `av1Support: true` in their join message; otherwise the VP8 track is forwarded. The sharer therefore publishes **both** a VP8 and an AV1 encoding of the same screen (`webapp/src/client.ts:1031-1040`).

---

## 5. Screenshare / mute / VAD

### 5.1 Screenshare start — order matters

`rtcd/client/api.go:75-154`. The `screen_on` WS message carries the **MediaStream ID** and **must be sent before the track arrives**, because the SFU classifies an incoming video track purely by comparing its `StreamID()` to the previously announced `screenStreamID` / `videoStreamID`:

```go
data, err := json.Marshal(map[string]string{ "screenStreamID": tracks[0].StreamID() })
...
if err := c.sendWS(wsEventScreenOn, map[string]any{"data": string(data)}, false); err != nil { ... }

trx, err := c.pc.AddTransceiverFromTrack(tracks[0], webrtc.RTPTransceiverInit{Direction: webrtc.RTPTransceiverDirectionSendonly})
```
SFU side (`rtcd/service/rtc/server.go:257-272` and `sfu.go:549-564`):
```go
case ScreenOnMessage:
	data := map[string]string{}
	json.Unmarshal(msg.Data, &data)
	session.mut.Lock(); session.screenStreamID = data["screenStreamID"]; session.mut.Unlock()
	if ok := call.setScreenSession(session); !ok { s.log.Error("screen session should not be set") }
```
```go
isScreen := screenStreamID != "" && screenStreamID == streamID
if isScreen { outTrackType = trackTypeScreen }
else if videoStreamID != "" && videoStreamID == streamID { outTrackType = trackTypeVideo }
else { s.log.Error("received unexpected video track", ...); return }   // <-- track is DROPPED
```
An **audio** track whose stream ID equals `screenStreamID` becomes `screen-audio` instead of `voice` (`sfu.go:432-437`).

### 5.2 Screenshare stop

`rtcd/client/api.go:156-169`: `pc.RemoveTrack(trx.Sender())` for each screen transceiver (triggers renegotiation) **then** `screen_off`. Server clears state via `call.clearScreenState` and pushes `trackActionRemove` to every other session (`rtc/call.go:92-126`). Plugin also clears `ScreenSharingSessionID` and broadcasts `user_screen_off`.

### 5.3 Mute — **both**, depending on client

There is no single answer; the protocol supports two mechanisms and the SFU implements a third layer:

1. **Signaling flag → SFU-side RTP gate.** `mute`/`unmute` sets `outVoiceTrackEnabled`, and the SFU simply *drops* your RTP instead of forwarding it (`rtcd/service/rtc/server.go:290-317` and `sfu.go:527-534`):
   ```go
   if trackType == trackTypeVoice {
       us.mut.RLock(); isEnabled := us.outVoiceTrackEnabled; us.mut.RUnlock()
       if !isEnabled { continue }
   }
   ```
   Muting also resets the VAD monitor (`server.go:300-308`).
2. **`RTPSender` manipulation (native client)** — `rtcd/client/api.go:25-73`: `Unmute(track)` does `AddTrack` the first time (→ renegotiation) and `ReplaceTrack` thereafter, then sends `unmute`. `Mute()` only sends the WS message; the track keeps running and the SFU gates it.
3. **`replaceTrack(null)` (browser)** — `webapp/src/client.ts:887-945`: mute = `peer.replaceTrack(audioTrack.id, null)` + `track.enabled = false` + `ws.send('mute')`; unmute = `addTrack` on first use (renegotiation), `replaceTrack` afterwards, + `ws.send('unmute')`. `RTCPeer.replaceTrack` explicitly avoids the lock: *"Since we expect replaceTrack not to cause a re-negotiation, locking is not required"* (`rtc_peer.ts:478`).

**Recommendation for Rust**: mirror #2 — add the Opus track once on first unmute, thereafter toggle by `replace_track(None/Some)`, and always send the `mute`/`unmute` WS message so the SFU gate and the UI stay in sync.

Note `outVoiceTrackEnabled` is initialised to `true` when the SFU first sees your audio track (`sfu.go:449`), while the plugin's stored `unmuted` starts `false`. Send `unmute` explicitly.

### 5.4 `voice_on` / `voice_off` are server-generated

The SFU runs a VAD over the `ssrc-audio-level` RTP header extension (`sfu.go:484-525`, `rtc/session.go:585-611`, `rtc/vad/vad.go`). Defaults: 50-sample window, activation threshold 10, deactivation 4, activation duration 2 s. Transitions produce `rtc.VoiceOnMessage`/`VoiceOffMessage`, which the plugin converts to `user_voice_on`/`user_voice_off` WS events (`server/websocket.go:662-680`). **Your client must negotiate `urn:ietf:params:rtp-hdrext:ssrc-audio-level` and populate it**, or you will never appear as "speaking" to anyone.

### 5.5 Video (camera)

Symmetric to screenshare but with `video_on` / `{"videoStreamID":"…"}`, relayed as `video_*` tracks. `VideoOffMessage` is a **no-op in the SFU** (`rtc/server.go:289`); the video sender is kept alive and the track replaced with `null` to avoid renegotiation (`webapp/src/client.ts:1172-1199`). Gated by `EnableVideo` and DM channels only.

---

## 6. End-to-end join sequence

Derived from `rtcd/client/client.go:182-197`, `websocket.go:111-406`, `rtc.go:188-539`, `call.go`, cross-checked with the SFU's `handleDCNegotiation` (`rtcd/service/rtc/session.go:146-210`) and `handleTracks` (`sfu.go:812-949`).

**Phase A — REST bootstrap (all awaited, order flexible except 4)**

1. `GET /plugins/com.mattermost.calls/version` → decide `dcLocking`.
2. `GET /plugins/com.mattermost.calls/config` → `ICEServersConfigs`, `EnableAV1`, `EnableDCSignaling`, `EnableSimulcast`, `NeedsTURNCredentials`, `AllowScreenSharing`, `MaxCallParticipants`.
3. *(optional)* `GET /plugins/com.mattermost.calls/{channelID}` or `/calls/{channelID}/active` → existing call state.
4. **If** `NeedsTURNCredentials`: `GET /plugins/com.mattermost.calls/turn-credentials` → append to the ICE list. Must complete before creating the `RTCPeerConnection`.

**Phase B — WebSocket + join handshake**

5. Open `wss://host/api/v4/websocket` with `Authorization: Bearer <token>`. *(For reconnect: `?connection_id=<prev>&sequence_number=<lastSeq>`.)*
6. **Await** the `hello` event → `data.connection_id`. **Store it as `originalConnID` — this is your session ID for the whole call.** (`rtcd/client/websocket.go:111-136`)
7. Send (text/JSON):
   ```json
   {"action":"custom_com.mattermost.calls_join","seq":1,
    "data":{"channelID":"…26…","jobID":"","av1Support":false,"dcSignaling":true}}
   ```
   Triggered by the hello handler: `if !isReconnect { c.joinCall() }` (`websocket.go:172-176`).
8. **Await** `custom_com.mattermost.calls_join` with `data.connID == originalConnID` — the join ACK. Do **not** create the PeerConnection before this. (`websocket.go:177-181` → `initRTCSession()`.)
   You will also receive, around this point: `call_start` (if you started it), `user_joined` for yourself, and `call_state` with the full roster.
   *(Failure path: `custom_..._error` with `data.data` = reason, e.g. `"forbidden"`, `"no call ongoing"`, group-calls-not-allowed, participant limit.)*

**Phase C — PeerConnection + initial negotiation**

9. Create the PeerConnection (Unified Plan, ICE servers from steps 2/4, SCTP zero-checksum, UDP4+TCP4). Register `on_ice_candidate`, `on_track`, `on_negotiation_needed`, `on_ice_connection_state_change` **before** step 10.
10. Create the data channel **`calls-dc`**. This is what makes negotiation "needed". (`rtc.go:424-430`)
11. `create_offer()` → `set_local_description()` → send `sdp` as a **binary msgpack** frame with zlib-compressed JSON (§3.1). *(First negotiation: no lock.)*
12. Trickle: for each local ICE candidate, send `ice` with `{"data": "<json of candidate init>"}`. Skip the `nil` end-of-candidates.
13. **Await** `custom_..._signal` with `{"type":"answer","sdp":"…"}` → `set_remote_description`. Then flush any remote candidates you queued. Set `dc_negotiated = true` (do **not** send `Unlock` for the first answer).
    - SFU: `handleDCNegotiation` had been blocking on this offer with a 10 s timeout; it now starts `handleICE` and waits for the DC to open.
14. Concurrently, `custom_..._signal` `{"type":"candidate","candidate":{...}}` messages arrive. If `remote_description` is not set yet, **queue them** (`rtc.go:77-86`), otherwise `add_ice_candidate` immediately.
15. ICE completes → DTLS → SCTP → `calls-dc` opens on both ends. Start the 1 Hz DC `Ping`. The SFU's `dcOpenCh` fires and it calls `handleTracks` (`session.go:199-207`).
16. The SFU now offers you **one renegotiation per already-present remote track** (`sfu.go:812-874` enumerates every other session's `outVoiceTrack`, screen, screen-audio, video). Each is: `MediaMap` on the DC, then an offer (DC if `dcSignaling`, else WS `signal`), and it waits up to 10 s for your answer on `sdpAnswerInCh`. For each: `set_remote_description(offer)` → `create_answer` → `set_local_description` → send SDP back on the same transport → then `Unlock` (post-first-negotiation). **Remote audio is now flowing in.**

**Phase D — start sending audio**

17. Create your Opus track (48 kHz, stereo, `minptime=10;useinbandfec=1`), negotiate `ssrc-audio-level`.
18. Take the signaling lock: send DC `Lock` (byte `0x07`), await DC `Lock`+`true` (retry on `false` every 100 ms, 5 s deadline). *(Skip if `dcLocking` is false or you have no DC.)*
19. `add_track(opus)` → `create_offer` → `set_local_description` → send `sdp` (DC if open, else WS binary).
20. **Await** the `answer` → `set_remote_description` → send DC `Unlock`.
21. Send `{"action":"custom_com.mattermost.calls_unmute","seq":N}`.
    - Ordering note: `rtcd/client/api.go:25-66` does `AddTrack` **then** `sendWS(unmute)`; the SFU only gates on `outVoiceTrackEnabled` so either order works, but this one avoids a "muted user is audible" window.
22. Start writing RTP. **Audio now flows both ways.** Others receive `user_unmuted` and, once you speak, `user_voice_on`.

**Ongoing**
- DC `Ping` every 1 s; the SFU replies `Pong`; report `LossRate`/`RoundTripTime`/`Jitter` on the DC every ~4 s (Go client, `rtcMonitorInterval`) or 10 s (webapp).
- New participant joins → `user_joined` + SFU-initiated offer(s) for their tracks.
- Participant leaves → `user_left`; stop the receivers you registered under that `session_id` (`rtcd/client/websocket.go:191-205`).
- `call_end` for your channel → tear down (`websocket.go:206-214`).

**Leave**: send `{"action":"custom_com.mattermost.calls_leave"}` then close the WS (`rtcd/client/websocket.go:356-362`, `webapp/src/websocket.ts:193-212`). If you just drop the socket, the plugin waits `wsReconnectionTimeout = 10s` for a `reconnect` before tearing down (`server/websocket.go:692-726`).

**Reconnect**: on WS close, reopen (backoff 1 s + jitter, give up after 30 s), await `hello`, then send `reconnect` with `{channelID, originalConnID, prevConnID}` instead of `join` — the RTC session and all media survive (`rtcd/client/websocket.go:332-354, 364-397`; plugin `handleReconnect` at `server/websocket.go:1057-1158`).

---

## 7. Rust porting notes — serde shapes for every `rtcd/client/types.go` type

`types.go` is small; the real surface is `types.go` + the WS envelope + the DC framing. All of it below.

**Crates**: `rmp-serde` (msgpack — **`to_vec_named`, never `to_vec`**, because MM decodes into `map[string]any`), `serde_json`, `flate2` (**`ZlibEncoder`/`ZlibDecoder`, i.e. RFC1950 with the 2-byte header + Adler-32, not `DeflateEncoder`**), `tokio-tungstenite`, `webrtc` (webrtc-rs).

### 7.1 `types.go` direct ports

```rust
pub const PLUGIN_ID: &str = "com.mattermost.calls";
pub const WS_EV_PREFIX: &str = "custom_com.mattermost.calls_";

/// rtcd/client/types.go:8-13
#[derive(Serialize, Debug, Clone, Default)]
pub struct CallJoinMessage {
    #[serde(rename = "channelID")]   pub channel_id: String,
    #[serde(rename = "jobID")]       pub job_id: String,        // "" for normal users
    #[serde(rename = "av1Support")]  pub av1_support: bool,
    #[serde(rename = "dcSignaling")] pub dc_signaling: bool,
    // plugin also accepts (webapp sends these; Go client does not):
    #[serde(rename = "title",    skip_serializing_if = "Option::is_none")] pub title: Option<String>,
    #[serde(rename = "threadID", skip_serializing_if = "Option::is_none")] pub thread_id: Option<String>,
}

/// rtcd/client/types.go:15-19
#[derive(Serialize, Debug, Clone)]
pub struct CallReconnectMessage {
    #[serde(rename = "channelID")]      pub channel_id: String,
    #[serde(rename = "originalConnID")] pub original_conn_id: String,
    #[serde(rename = "prevConnID")]     pub prev_conn_id: String,
}

/// rtcd/client/types.go:21-27  (arrives as JSON inside call_job_state.jobState)
#[derive(Deserialize, Debug, Clone, Default)]
pub struct CallJobState {
    #[serde(rename = "type")]     pub job_type: String,   // "recording" | "transcribing" | "captioning"
    #[serde(rename = "init_at")]  pub init_at: i64,
    #[serde(rename = "start_at")] pub start_at: i64,
    #[serde(rename = "end_at")]   pub end_at: i64,
    #[serde(default, rename = "err")] pub err: String,
}

/// rtcd/client/types.go:43-47
pub const TRACK_TYPE_VOICE: &str = "voice";
pub const TRACK_TYPE_SCREEN: &str = "screen";
pub const TRACK_TYPE_VIDEO: &str = "video";
pub const TRACK_TYPE_SCREEN_AUDIO: &str = "screen-audio"; // SFU-only const, but you WILL receive it

/// rtcd/client/utils.go:13-20
pub fn parse_track_id(track_id: &str) -> Option<(&str, &str)> {
    let mut it = track_id.splitn(3, '_');
    let (t, sid, rest) = (it.next()?, it.next()?, it.next()?);
    if rest.is_empty() { return None; }   // Go requires >= 3 fields
    Some((t, sid))
}

/// rtcd/client/config.go:15-35
pub struct Config {
    pub site_url: String,            // trailing '/' trimmed; http|https only
    pub auth_token: String,
    pub channel_id: String,          // must match ^[a-z0-9]{26}$
    pub job_id: String,
    pub enable_av1: bool,
    pub enable_dc_signaling: bool,
    pub enable_rtc_monitor: bool,
    // derived: ws_url = site_url with http->ws / https->wss + "/api/v4/websocket"
}
```

### 7.2 WebSocket envelope

```rust
/// mattermost/server/public/model/websocket_request.go:18-28
#[derive(Serialize)]
pub struct WsRequest<T: Serialize> {
    pub action: String,   // WS_EV_PREFIX + "join" etc.
    pub seq: i64,         // monotonically increasing from 1
    pub data: T,
}

/// public/model/websocket_message.go:234-239
#[derive(Deserialize, Debug)]
pub struct WsEvent {
    pub event: String,
    #[serde(default)] pub data: serde_json::Map<String, serde_json::Value>,
    #[serde(default)] pub broadcast: Option<WsBroadcast>,
    #[serde(default)] pub seq: i64,
    #[serde(default)] pub seq_reply: Option<i64>,   // present on ping/ack replies
}

#[derive(Deserialize, Debug, Default)]
pub struct WsBroadcast {
    #[serde(default)] pub omit_users: Option<HashMap<String, bool>>,
    #[serde(default)] pub user_id: String,
    #[serde(default)] pub channel_id: String,
    #[serde(default)] pub team_id: String,
    #[serde(default)] pub connection_id: String,
    #[serde(default)] pub omit_connection_id: String,
}
```

Send helpers (mirrors `rtcd/client/websocket.go:73-102`):

```rust
fn send_json<T: Serialize>(ws, ev: &str, data: T, seq: &mut i64) -> Result<()> {
    let req = WsRequest { action: ev.into(), seq: *seq, data };
    *seq += 1;
    ws.send(Message::Text(serde_json::to_string(&req)?))
}

fn send_msgpack<T: Serialize>(ws, ev: &str, data: T, seq: &mut i64) -> Result<()> {
    let req = WsRequest { action: ev.into(), seq: *seq, data };
    *seq += 1;
    // MUST be map-encoded (named) — MM decodes into map[string]any
    ws.send(Message::Binary(rmp_serde::to_vec_named(&req)?))
}
```

For `data: null` messages (mute, leave, raise_hand…) use `Option::<()>::None` — Go sends `"data": null` and the plugin's cases for those types never look at `data`.

### 7.3 Signaling payloads

```rust
#[derive(Serialize)]
pub struct SdpWsData {
    #[serde(with = "serde_bytes")]     // <-- CRITICAL: msgpack `bin`, not array-of-ints
    pub data: Vec<u8>,                 // zlib(json({"type":..,"sdp":..}))
}

#[derive(Serialize)]
pub struct IceWsData { pub data: String }   // JSON string of RTCIceCandidateInit

#[derive(Serialize)]
pub struct ScreenOnData { pub data: String } // "{\"screenStreamID\":\"...\"}"
#[derive(Serialize)]
pub struct VideoOnData  { pub data: String } // "{\"videoStreamID\":\"...\"}"

/// The inner object of the `signal` event (data.data, a JSON string)
#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SignalMsg {
    Offer  { sdp: String },
    Answer { sdp: String },
    Candidate { candidate: RTCIceCandidateInit },   // NOTE the extra nesting level
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct RTCIceCandidateInit {
    pub candidate: String,
    #[serde(default, rename = "sdpMid")]            pub sdp_mid: Option<String>,
    #[serde(default, rename = "sdpMLineIndex")]     pub sdp_mline_index: Option<u16>,
    #[serde(default, rename = "usernameFragment")]  pub username_fragment: Option<String>,
}
```

`serde_bytes` on the SDP field is mandatory: without it `rmp-serde` emits a msgpack *array* and the plugin's `req.Data["data"].([]byte)` assertion fails, dropping your offer with no error.

### 7.4 Data-channel codec

```rust
#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(u8)]
pub enum DcMessageType {
    Ping = 1, Pong = 2, Sdp = 3, LossRate = 4,
    RoundTripTime = 5, Jitter = 6, Lock = 7, Unlock = 8, MediaMap = 9,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrackInfo {
    #[serde(rename = "type")]      pub track_type: String,  // voice|screen|screen-audio|video
    #[serde(rename = "sender_id")] pub sender_id: String,   // UNRELIABLE — see §4.4
}
pub type MediaMap = HashMap<String, TrackInfo>;   // key = transceiver mid

/// Flat framing: 1 type byte (msgpack positive fixint) + optional msgpack payload.
pub fn encode_dc(mt: DcMessageType, payload: Option<DcPayload>) -> Result<Vec<u8>> {
    let mut buf = vec![mt as u8];               // values 1..=9 -> positive fixint == raw byte
    match payload {
        None => {}
        Some(DcPayload::Sdp(json)) => {
            let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
            e.write_all(json.as_bytes())?;
            let z = e.finish()?;
            buf.extend(rmp_serde::to_vec(&serde_bytes::Bytes::new(&z))?);  // msgpack bin
        }
        Some(DcPayload::F64(v))  => buf.extend(rmp_serde::to_vec(&v)?),
        Some(DcPayload::Bool(b)) => buf.extend(rmp_serde::to_vec(&b)?),
        Some(DcPayload::Map(m))  => buf.extend(rmp_serde::to_vec_named(&m)?),
    }
    Ok(buf)
}

pub fn decode_dc(data: &[u8]) -> Result<(DcMessageType, Option<DcPayload>)> {
    let mt = DcMessageType::try_from(data[0])?;    // Go: dec.DecodeUint8()
    let rest = &data[1..];
    Ok(match mt {
        DcMessageType::Ping | DcMessageType::Pong | DcMessageType::Unlock => (mt, None),
        DcMessageType::Sdp => {
            let z: serde_bytes::ByteBuf = rmp_serde::from_slice(rest)?;
            let mut s = String::new();
            ZlibDecoder::new(&z[..]).read_to_string(&mut s)?;
            (mt, Some(DcPayload::Json(s)))          // {"type":"offer","sdp":"..."}
        }
        DcMessageType::LossRate | DcMessageType::RoundTripTime | DcMessageType::Jitter =>
            (mt, Some(DcPayload::F64(rmp_serde::from_slice(rest)?))),
        DcMessageType::Lock => // payload present only on responses (Go falls back to None on error)
            (mt, rmp_serde::from_slice::<bool>(rest).ok().map(DcPayload::Bool)),
        DcMessageType::MediaMap =>
            (mt, Some(DcPayload::Map(rmp_serde::from_slice::<MediaMap>(rest)?))),
    })
}
```

### 7.5 State / event structs

```rust
/// server/state.go:82-88
#[derive(Deserialize, Debug, Clone)]
pub struct UserStateClient {
    pub session_id: String, pub user_id: String,
    pub unmuted: bool, pub raised_hand: i64,
    #[serde(default)] pub video: bool,
}

/// server/state.go:90-106 — arrives as a JSON *string* in call_state.data.call
#[derive(Deserialize, Debug, Clone)]
pub struct CallStateClient {
    pub id: String, pub start_at: i64,
    pub sessions: Vec<UserStateClient>,
    pub thread_id: String, pub post_id: String,
    pub screen_sharing_session_id: String,
    pub owner_id: String, pub host_id: String,
    #[serde(default)] pub recording: Option<CallJobState>,
    #[serde(default)] pub transcription: Option<CallJobState>,
    #[serde(default)] pub live_captions: Option<CallJobState>,
    #[serde(default)] pub dismissed_notification: Option<HashMap<String, bool>>,
}

/// GET /config — server/configuration.go:97-140 (no json tags => Go field names verbatim)
#[derive(Deserialize, Debug, Clone, Default)]
#[allow(non_snake_case)]
pub struct ClientConfig {
    #[serde(default)] pub ICEServers: Vec<String>,
    #[serde(default)] pub ICEServersConfigs: Vec<IceServerConfig>,
    #[serde(default)] pub AllowEnableCalls: Option<bool>,
    #[serde(default)] pub DefaultEnabled: Option<bool>,
    #[serde(default)] pub MaxCallParticipants: Option<i64>,
    #[serde(default)] pub NeedsTURNCredentials: Option<bool>,
    #[serde(default)] pub AllowScreenSharing: Option<bool>,
    #[serde(default)] pub EnableRecordings: Option<bool>,
    #[serde(default)] pub EnableTranscriptions: Option<bool>,
    #[serde(default)] pub EnableLiveCaptions: Option<bool>,
    #[serde(default)] pub MaxRecordingDuration: Option<i64>,
    #[serde(default)] pub EnableSimulcast: Option<bool>,
    #[serde(default)] pub EnableRinging: Option<bool>,
    #[serde(default)] pub sku_short_name: String,
    #[serde(default)] pub HostControlsAllowed: bool,
    #[serde(default)] pub EnableAV1: Option<bool>,
    #[serde(default)] pub GroupCallsAllowed: bool,
    #[serde(default)] pub EnableDCSignaling: Option<bool>,
    #[serde(default)] pub EnableVideo: Option<bool>,
}

/// rtcd/service/rtc/config.go:175-179 — lowercase tags here
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub credential: String,
}

/// GET /version — server/public/version.go:6-11
#[derive(Deserialize, Debug, Clone, Default)]
pub struct VersionInfo {
    pub version: String, pub build: String,
    #[serde(default)] pub rtcd_version: String,
    #[serde(default)] pub rtcd_build: String,
}
```

### 7.6 Constants worth hoisting

```rust
const MM_WS_PATH: &str            = "/api/v4/websocket";   // rtcd/client/websocket.go:23
const WS_MAX_FRAME_BYTES: usize   = 8 * 1024;              // model.SocketMaxMessageSizeKb
const SDP_MAX_DECOMPRESSED: usize = 64 * 1024;             // server/utils.go:32
const SIGNALING_TIMEOUT: Duration        = Duration::from_secs(10);   // rtc/server.go:23
const SIGNALING_LOCK_TIMEOUT: Duration   = Duration::from_secs(5);    // rtc/session.go:30
const DC_PING_INTERVAL: Duration         = Duration::from_secs(1);    // rtcd/client/rtc.go:33
const RTC_MONITOR_INTERVAL: Duration     = Duration::from_secs(4);    // rtcd/client/rtc.go:32
const WS_RECONNECT_TIMEOUT: Duration     = Duration::from_secs(30);   // rtcd/client/websocket.go:69
const PLUGIN_RECONNECT_GRACE: Duration   = Duration::from_secs(10);   // server/websocket.go:57
const RECEIVE_MTU: usize                 = 1460;                      // rtcd/client/rtc.go:31
const ICE_QUEUE_SIZE: usize              = 20;                        // rtcd/client/rtc.go:30
const WS_MSG_RATE: (f64, usize)          = (10.0, 100);               // server/session.go:76
```

### 7.7 webrtc-rs gotchas

- Register **only** Opus/111, VP8/96, AV1/45 in your `MediaEngine` if you want a clean SDP; the SFU will reject anything else anyway.
- Register `urn:ietf:params:rtp-hdrext:ssrc-audio-level` for audio (needed for VAD) and the three `sdes:mid`/`rtp-stream-id`/`repaired-rtp-stream-id` extensions for video (`rtcd/client/rtc.go:36-40`).
- `SettingEngine::enable_sctp_zero_checksum(true)` and restrict network types to UDP4/TCP4 to match the reference client.
- Read RTCP off every `RTPReceiver` and `RTPSender` in a background task or pion-side interceptors starve (`rtcd/client/rtc.go:328-345`, `api.go:52-63`).
- Send a PLI immediately on receiving a `screen` track (`rtc.go:317-322`); the SFU rate-limits PLI forwarding to 1/s per SSRC (`rtc/session.go:340-358`) and will retry-tolerate you.
- Stop receivers for a `session_id` on `user_left` (`websocket.go:191-205`).

---

## 8. Things I could NOT verify / explicit caveats

1. **`sfu_url`, `ws_url`, `need_upgrade` do not exist** in any of the three repos (exhaustive grep). The WS URL is *derived* client-side from the MM site URL; `RTCDServiceURL` is an admin-only config field never sent to clients. If your spec came from an older Calls version, treat these as removed.
2. **`user_connected` / `user_disconnected` do not exist** as WS events here. `calls-common/src/types/types.ts:29-35` still declares the TS types, but nothing publishes or registers them. Current equivalents: `user_joined` / `user_left` (session-scoped).
3. **No dedicated ringing/dismissed signaling messages.** Ringing is `call_start` + `EnableRinging` client behaviour; dismissal is `POST /calls/{channelID}/dismiss-notification` → `user_dismissed_notification` WS event (to the dismissing user only). Push notifications for ringing exist in `server/push_notifications.go` — I did not read that file in detail, so the exact push payload is **unverified**.
4. **`MediaMap.sender_id` is wrong** (`rtcd/service/rtc/session.go:655-658` sets it to the *receiving* session's ID). No shipping client reads it. Use `parse_track_id(track.id())`.
5. **Pion's `RegisterDefaultCodecs` / `RegisterDefaultInterceptors` contents** — the pion module source is not present in these checkouts, so I could not enumerate exactly which codecs/extensions the reference Go client offers. What matters is the *intersection*, and the SFU's `MediaEngine` (`sfu.go:129-156`) is fully verified: Opus/111, VP8/96, AV1/45, audio-level ext, mid/rid/rrid exts, TWCC.
6. **`voice_on`/`voice_off` as client→server actions** are listed in `validClientMessageTypes` (`server/client_message.go:53-54`) but have **no handler branch** in `handleClientMsg`, so they hit the `default:` case and return `invalid client message type`. I did not find any code path that would make them work; treat them as server→client only.
7. **Cluster/HA relaying** (`clusterMessageType*`, `sendClusterMessage`) is internal to multi-node MM and transparent to clients. I read enough to confirm it doesn't change the wire format, but did not audit `server/cluster_message.go` in full.
8. **The exact zlib compression *level* used by the peers** is Go's `zlib.NewWriter` default (level 6) and fflate's `zlibSync` default. Decompression is level-agnostic, so this only matters if you're matching byte-for-byte fixtures.
9. I did not exercise anything at runtime — this is all static reading. The strongest runtime evidence available is `rtcd/client/rtc_test.go` and `api_test.go`, which are integration tests against a real MM+plugin and which I used to confirm track-ID format and payload types.
agentId: ac62f0dbd968aa0cb (use SendMessage with to: 'ac62f0dbd968aa0cb', summary: '<5-10 word recap>' to continue this agent)
<usage>subagent_tokens: 320811
tool_uses: 90
duration_ms: 923339</usage>