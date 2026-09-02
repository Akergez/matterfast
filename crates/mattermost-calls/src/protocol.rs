//! Wire protocol for Mattermost Calls.
//!
//! A third-party client **never talks to the SFU's signalling directly**. It
//! talks to Mattermost's ordinary `/api/v4/websocket`, sending plugin-namespaced
//! actions (`custom_com.mattermost.calls_*`). The plugin demuxes those to an
//! embedded SFU or an external `rtcd`. Media then flows peer-to-SFU over
//! DTLS/SRTP.
//!
//! The normative reference implementation is `rtcd/client/` (a headless Go
//! client), which is what these types were derived from.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const PLUGIN_ID: &str = "com.mattermost.calls";
/// Every Calls action and event is namespaced with this.
pub const WS_PREFIX: &str = "custom_com.mattermost.calls_";

/// Builds the full websocket action string for a Calls message.
pub fn action(name: &str) -> String {
    format!("{WS_PREFIX}{name}")
}

/// Strips the Calls prefix from an incoming event name, if present.
pub fn strip_prefix(event: &str) -> Option<&str> {
    event.strip_prefix(WS_PREFIX)
}

/// Client → server action names (append to [`WS_PREFIX`]).
pub mod client_action {
    pub const JOIN: &str = "join";
    pub const RECONNECT: &str = "reconnect";
    pub const LEAVE: &str = "leave";
    pub const CALL_STATE: &str = "call_state";
    /// Binary msgpack frame carrying zlib-compressed SDP.
    pub const SDP: &str = "sdp";
    pub const ICE: &str = "ice";
    pub const MUTE: &str = "mute";
    pub const UNMUTE: &str = "unmute";
    pub const SCREEN_ON: &str = "screen_on";
    pub const SCREEN_OFF: &str = "screen_off";
    pub const VIDEO_ON: &str = "video_on";
    pub const VIDEO_OFF: &str = "video_off";
    pub const RAISE_HAND: &str = "raise_hand";
    pub const UNRAISE_HAND: &str = "unraise_hand";
    pub const REACT: &str = "react";
    pub const METRIC: &str = "metric";
}

/// Host-control routes, `POST /calls/{call_id}/host/{action}`.
///
/// Host controls are the one part of the protocol with **no websocket action**:
/// the plugin serves them over HTTP only, so they do not belong in
/// [`client_action`]. The server answers by emitting the matching `host_*`
/// event from [`server_event`] to the target, who is expected to obey it —
/// nothing is enforced in the media plane, so a client that ignores `host_mute`
/// keeps being heard.
pub mod host_route {
    pub const MUTE: &str = "mute";
    /// Takes no target: the plugin mutes everyone *except the caller*.
    pub const MUTE_OTHERS: &str = "mute-others";
    pub const SCREEN_OFF: &str = "screen-off";
    pub const LOWER_HAND: &str = "lower-hand";
    pub const REMOVE: &str = "remove";
    /// Body is `{"new_host_id": <user id>}` — a user id, not a session id.
    pub const MAKE: &str = "make";
    pub const END: &str = "end";
}

/// Server → client event names (already stripped of [`WS_PREFIX`]).
pub mod server_event {
    /// Your join was accepted; `data.connID` echoes your session id.
    pub const JOIN: &str = "join";
    pub const ERROR: &str = "error";
    /// SDP or ICE from the SFU. `data.data` is an uncompressed JSON string.
    pub const SIGNAL: &str = "signal";
    pub const CALL_START: &str = "call_start";
    pub const CALL_STATE: &str = "call_state";
    pub const CALL_END: &str = "call_end";
    pub const USER_JOINED: &str = "user_joined";
    pub const USER_LEFT: &str = "user_left";
    pub const USER_MUTED: &str = "user_muted";
    pub const USER_UNMUTED: &str = "user_unmuted";
    pub const USER_VOICE_ON: &str = "user_voice_on";
    pub const USER_VOICE_OFF: &str = "user_voice_off";
    pub const USER_SCREEN_ON: &str = "user_screen_on";
    pub const USER_SCREEN_OFF: &str = "user_screen_off";
    pub const USER_VIDEO_ON: &str = "user_video_on";
    pub const USER_VIDEO_OFF: &str = "user_video_off";
    pub const USER_RAISE_HAND: &str = "user_raise_hand";
    pub const USER_UNRAISE_HAND: &str = "user_unraise_hand";
    pub const USER_REACTED: &str = "user_reacted";
    pub const CALL_HOST_CHANGED: &str = "call_host_changed";
    pub const CALL_JOB_STATE: &str = "call_job_state";
    pub const USER_DISMISSED_NOTIFICATION: &str = "user_dismissed_notification";
    pub const HOST_MUTE: &str = "host_mute";
    pub const HOST_SCREEN_OFF: &str = "host_screen_off";
    pub const HOST_LOWER_HAND: &str = "host_lower_hand";
    pub const HOST_REMOVED: &str = "host_removed";
}

/// Relayed track types. The SFU renames every track it forwards to
/// `"{type}_{senderSessionID}_{8 hex}"` — see [`parse_track_id`].
pub mod track_type {
    pub const VOICE: &str = "voice";
    pub const SCREEN: &str = "screen";
    pub const SCREEN_AUDIO: &str = "screen-audio";
    pub const VIDEO: &str = "video";
}

/// The data channel the client opens; creating it is what triggers the first
/// negotiation.
pub const DATA_CHANNEL_LABEL: &str = "calls-dc";

/// Mattermost drops websocket frames larger than this
/// (`model.SocketMaxMessageSizeKb`). It is the reason SDP is zlib-compressed.
pub const WS_MAX_FRAME_BYTES: usize = 8 * 1024;
/// The plugin refuses SDP that decompresses to more than this.
pub const SDP_MAX_DECOMPRESSED: usize = 64 * 1024;

/// Splits an SFU-relayed track id into `(track_type, sender_session_id)`.
///
/// The SFU builds ids as `genTrackID(type, sessionID) = "{type}_{sessionID}_{rand8}"`,
/// so this is the **only** reliable way to attribute an incoming track to a
/// participant — there is no user id anywhere in the media plane. Map the
/// session id to a user id using `user_joined` / `call_state`.
///
/// Note `"screen-audio"` contains a hyphen but no underscore, so it splits
/// correctly.
pub fn parse_track_id(track_id: &str) -> Option<(&str, &str)> {
    let mut parts = track_id.splitn(3, '_');
    let kind = parts.next()?;
    let session = parts.next()?;
    let rest = parts.next()?;
    if kind.is_empty() || session.is_empty() || rest.is_empty() {
        return None;
    }
    Some((kind, session))
}

// ------------------------------------------------------------------ messages

/// `custom_com.mattermost.calls_join` payload.
#[derive(Debug, Clone, Default, Serialize)]
pub struct JoinMessage {
    #[serde(rename = "channelID")]
    pub channel_id: String,
    /// Bot-only; must be empty for a normal user.
    #[serde(rename = "jobID")]
    pub job_id: String,
    /// Advertises that we can *receive* AV1 screen share. Mutually exclusive
    /// with simulcast in practice.
    #[serde(rename = "av1Support")]
    pub av1_support: bool,
    /// Opt into data-channel signalling. Requires the server to have
    /// `EnableDCSignaling` on.
    #[serde(rename = "dcSignaling")]
    pub dc_signaling: bool,
    /// Only meaningful when *starting* a call: titles the "call started" post.
    #[serde(rename = "title", skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "threadID", skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// `custom_com.mattermost.calls_reconnect` payload. All three fields are
/// required or the plugin drops the message.
#[derive(Debug, Clone, Serialize)]
pub struct ReconnectMessage {
    #[serde(rename = "channelID")]
    pub channel_id: String,
    /// The connection id we joined with — our stable session identity.
    #[serde(rename = "originalConnID")]
    pub original_conn_id: String,
    /// The connection id of the socket that just died.
    #[serde(rename = "prevConnID")]
    pub prev_conn_id: String,
}

/// `sdp` payload. Must be sent as a **binary msgpack** frame: the plugin does
/// `req.Data["data"].([]byte)`, and a JSON frame would deliver a base64 string
/// instead, failing the type assertion *silently*.
#[derive(Debug, Clone, Serialize)]
pub struct SdpPayload {
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// `ice` payload — a JSON **string**, not compressed, not an object.
#[derive(Debug, Clone, Serialize)]
pub struct StringPayload {
    pub data: String,
}

/// `webrtc.SessionDescription` as it travels on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescription {
    /// `offer` | `answer` | `pranswer` | `rollback`
    #[serde(rename = "type")]
    pub kind: String,
    pub sdp: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IceCandidateInit {
    pub candidate: String,
    #[serde(default, rename = "sdpMid", skip_serializing_if = "Option::is_none")]
    pub sdp_mid: Option<String>,
    #[serde(
        default,
        rename = "sdpMLineIndex",
        skip_serializing_if = "Option::is_none"
    )]
    pub sdp_mline_index: Option<u16>,
    #[serde(
        default,
        rename = "usernameFragment",
        skip_serializing_if = "Option::is_none"
    )]
    pub username_fragment: Option<String>,
}

/// The inner object of a `signal` event (`data.data`, itself a JSON string).
///
/// Note the asymmetry: for `candidate` the init object is nested one level
/// deeper under `"candidate"`, because the SFU builds it as
/// `{"type":"candidate","candidate":c.ToJSON()}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Signal {
    Offer { sdp: String },
    Answer { sdp: String },
    Pranswer { sdp: String },
    Candidate { candidate: IceCandidateInit },
}

impl Signal {
    pub fn parse(json: &str) -> Result<Signal, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// One participant, as reported in `call_state`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub user_id: String,
    #[serde(default)]
    pub unmuted: bool,
    /// Millisecond timestamp; `0` means the hand is down.
    #[serde(default)]
    pub raised_hand: i64,
    #[serde(default)]
    pub video: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct JobState {
    #[serde(rename = "type", default)]
    pub job_type: String,
    #[serde(default)]
    pub init_at: i64,
    #[serde(default)]
    pub start_at: i64,
    #[serde(default)]
    pub end_at: i64,
    #[serde(default)]
    pub err: String,
}

/// The full call roster. Arrives as a JSON **string** inside
/// `call_state.data.call`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CallState {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub start_at: i64,
    #[serde(default)]
    pub sessions: Vec<SessionState>,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub post_id: String,
    #[serde(default)]
    pub screen_sharing_session_id: String,
    #[serde(default)]
    pub owner_id: String,
    #[serde(default)]
    pub host_id: String,
    #[serde(default)]
    pub recording: Option<JobState>,
    #[serde(default)]
    pub transcription: Option<JobState>,
    #[serde(default)]
    pub live_captions: Option<JobState>,
    #[serde(default)]
    pub dismissed_notification: Option<HashMap<String, bool>>,
}

impl CallState {
    pub fn participant_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn user_for_session(&self, session_id: &str) -> Option<&str> {
        self.sessions
            .iter()
            .find(|s| s.session_id == session_id)
            .map(|s| s.user_id.as_str())
    }

    /// Whether a user holds the host controls. The host is a *user*, not a
    /// session: a second device of the same user is host too.
    pub fn is_host(&self, user_id: &str) -> bool {
        !user_id.is_empty() && self.host_id == user_id
    }

    pub fn is_screen_sharing(&self) -> bool {
        !self.screen_sharing_session_id.is_empty()
    }
}

/// An emoji reaction sent during a call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CallReaction {
    pub name: String,
    #[serde(default)]
    pub skin: String,
    pub unified: String,
    #[serde(default)]
    pub literal: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_names_are_plugin_namespaced() {
        assert_eq!(
            action(client_action::JOIN),
            "custom_com.mattermost.calls_join"
        );
        assert_eq!(
            strip_prefix("custom_com.mattermost.calls_user_joined"),
            Some("user_joined")
        );
        assert_eq!(strip_prefix("posted"), None);
    }

    #[test]
    fn track_ids_split_into_type_and_session() {
        let sid = "kj3n8x2q1w9e7r5t4y6u8i0o1p";
        assert_eq!(
            parse_track_id(&format!("voice_{sid}_a1b2c3d4")),
            Some(("voice", sid))
        );
        // The hyphenated type must survive: it contains no underscore.
        assert_eq!(
            parse_track_id(&format!("screen-audio_{sid}_deadbeef")),
            Some(("screen-audio", sid))
        );
    }

    #[test]
    fn malformed_track_ids_are_rejected() {
        assert_eq!(parse_track_id("voice"), None);
        assert_eq!(parse_track_id("voice_abc"), None);
        assert_eq!(parse_track_id("voice_abc_"), None);
    }

    #[test]
    fn signal_candidate_is_nested_one_level_deeper_than_sdp() {
        let offer = Signal::parse(r#"{"type":"offer","sdp":"v=0\r\n"}"#).unwrap();
        assert!(matches!(offer, Signal::Offer { .. }));

        let cand = Signal::parse(
            r#"{"type":"candidate","candidate":{"candidate":"candidate:1 1 udp 1 10.0.0.1 8443 typ host","sdpMid":"0","sdpMLineIndex":0}}"#,
        )
        .unwrap();
        match cand {
            Signal::Candidate { candidate } => {
                assert!(candidate.candidate.starts_with("candidate:1"));
                assert_eq!(candidate.sdp_mid.as_deref(), Some("0"));
                assert_eq!(candidate.sdp_mline_index, Some(0));
            }
            other => panic!("expected Candidate, got {other:?}"),
        }
    }

    #[test]
    fn join_message_uses_the_servers_camel_case_field_names() {
        let msg = JoinMessage {
            channel_id: "c".into(),
            job_id: String::new(),
            av1_support: false,
            dc_signaling: true,
            title: None,
            thread_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(
            json,
            r#"{"channelID":"c","jobID":"","av1Support":false,"dcSignaling":true}"#
        );
    }

    #[test]
    fn sdp_payload_encodes_as_msgpack_bin_not_an_array() {
        let payload = SdpPayload {
            data: vec![0x78, 0x9c, 0x01],
        };
        let bytes = rmp_serde::to_vec_named(&payload).unwrap();
        // 0xc4 is msgpack `bin 8`; an array would start 0x9x. Getting this
        // wrong makes the plugin drop our offer with no error at all.
        assert!(
            bytes.windows(2).any(|w| w[0] == 0xc4 && w[1] == 3),
            "expected a bin8 header, got {bytes:02x?}"
        );
    }

    #[test]
    fn call_state_maps_sessions_to_users() {
        let raw = r#"{"id":"call1","start_at":1,"sessions":[
            {"session_id":"s1","user_id":"u1","unmuted":true,"raised_hand":0},
            {"session_id":"s2","user_id":"u2","unmuted":false,"raised_hand":123}],
            "thread_id":"","post_id":"","screen_sharing_session_id":"s2",
            "owner_id":"u1","host_id":"u1"}"#;
        let state: CallState = serde_json::from_str(raw).unwrap();
        assert_eq!(state.participant_count(), 2);
        assert_eq!(state.user_for_session("s2"), Some("u2"));
        assert!(state.is_screen_sharing());
        assert!(state.is_host("u1"));
        assert!(!state.is_host("u2"));
        // An empty user id must never come out as the host.
        assert!(!CallState::default().is_host(""));
    }
}
