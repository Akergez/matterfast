//! Turns raw plugin websocket events into typed Calls events.
//!
//! Beware the key casing: the plugin emits `user_id` on some events and
//! `userID` on others (`user_muted`, `user_voice_on`, `user_screen_on`,
//! `user_raise_hand` all use the camelCase form). [`user_id_of`] accepts both.

use mattermost_api::ws::{Broadcast, Data, Event};
use serde_json::Value;

use crate::protocol::{self, CallReaction, CallState, JobState};

fn s(data: &Data, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn i(data: &Data, key: &str) -> i64 {
    data.get(key).and_then(Value::as_i64).unwrap_or_default()
}

/// The plugin is inconsistent about this key; try both spellings.
fn user_id_of(data: &Data) -> String {
    let id = s(data, "user_id");
    if id.is_empty() {
        s(data, "userID")
    } else {
        id
    }
}

/// A typed Calls event.
#[derive(Debug, Clone)]
pub enum CallsEvent {
    /// Our join was accepted. `conn_id` echoes our own session id.
    JoinAccepted {
        conn_id: String,
    },
    /// The plugin refused: participant limit, unlicensed group call, no
    /// permission, no call ongoing…
    Error {
        message: String,
        conn_id: String,
    },
    /// SDP or ICE from the SFU. The payload is an *uncompressed* JSON string;
    /// only client→server SDP is zlib-compressed.
    Signal {
        signal: protocol::Signal,
        conn_id: String,
    },
    CallStarted {
        call_id: String,
        channel_id: String,
        start_at: i64,
        thread_id: String,
        post_id: String,
        owner_id: String,
        host_id: String,
    },
    /// The full roster, sent right after join and on membership changes.
    CallState {
        channel_id: String,
        state: Box<CallState>,
    },
    CallEnded {
        channel_id: String,
    },
    UserJoined {
        user_id: String,
        session_id: String,
        channel_id: String,
    },
    UserLeft {
        user_id: String,
        session_id: String,
        channel_id: String,
    },
    /// Muted state changed. Driven by the peer's own `mute`/`unmute` message.
    UserMuted {
        user_id: String,
        session_id: String,
        muted: bool,
    },
    /// Voice activity, detected **server-side** from the RTP audio-level header
    /// extension. You will never appear as speaking unless you negotiate and
    /// populate `urn:ietf:params:rtp-hdrext:ssrc-audio-level`.
    UserSpeaking {
        user_id: String,
        session_id: String,
        speaking: bool,
    },
    UserScreenShare {
        user_id: String,
        session_id: String,
        sharing: bool,
    },
    UserVideo {
        user_id: String,
        session_id: String,
        on: bool,
    },
    UserRaisedHand {
        user_id: String,
        session_id: String,
        /// Millisecond timestamp; `0` means the hand went down.
        raised_at: i64,
    },
    UserReacted {
        user_id: String,
        session_id: String,
        reaction: CallReaction,
        timestamp: i64,
    },
    HostChanged {
        host_id: String,
        call_id: String,
    },
    JobState {
        call_id: String,
        state: JobState,
    },
    /// A host asked us to mute ourselves. Host controls are **advisory**: the
    /// server does not force-mute, it expects the client to comply.
    HostMuteRequest {
        session_id: String,
    },
    HostScreenOffRequest {
        session_id: String,
    },
    HostLowerHandRequest {
        session_id: String,
    },
    HostRemoved {
        session_id: String,
        user_id: String,
    },
    /// A Calls event we do not model.
    Unhandled {
        name: String,
        data: Data,
    },
}

impl CallsEvent {
    /// The session id an event refers to, when it refers to one.
    pub fn session_id(&self) -> Option<&str> {
        use CallsEvent::*;
        Some(match self {
            UserJoined { session_id, .. }
            | UserLeft { session_id, .. }
            | UserMuted { session_id, .. }
            | UserSpeaking { session_id, .. }
            | UserScreenShare { session_id, .. }
            | UserVideo { session_id, .. }
            | UserRaisedHand { session_id, .. }
            | UserReacted { session_id, .. }
            | HostMuteRequest { session_id }
            | HostScreenOffRequest { session_id }
            | HostLowerHandRequest { session_id }
            | HostRemoved { session_id, .. } => session_id,
            _ => return None,
        })
    }
}

/// Attempts to interpret a generic websocket event as a Calls event.
///
/// Returns `None` for anything that is not plugin-namespaced, so callers can
/// pass every event through unconditionally.
pub fn parse(event: &Event) -> Option<CallsEvent> {
    let (name, data, broadcast) = match event {
        Event::Other {
            event,
            data,
            broadcast,
        } => (protocol::strip_prefix(event)?, data, broadcast),
        _ => return None,
    };
    Some(parse_named(name, data, broadcast))
}

fn parse_named(name: &str, d: &Data, b: &Broadcast) -> CallsEvent {
    use protocol::server_event as ev;

    let session_id = s(d, "session_id");
    let user_id = user_id_of(d);
    let channel_id = if d.contains_key("channelID") {
        s(d, "channelID")
    } else if d.contains_key("channel_id") {
        s(d, "channel_id")
    } else {
        b.channel_id.clone()
    };

    match name {
        ev::JOIN => CallsEvent::JoinAccepted {
            conn_id: s(d, "connID"),
        },
        ev::ERROR => CallsEvent::Error {
            message: s(d, "data"),
            conn_id: s(d, "connID"),
        },
        ev::SIGNAL => {
            let raw = s(d, "data");
            match protocol::Signal::parse(&raw) {
                Ok(signal) => CallsEvent::Signal {
                    signal,
                    conn_id: s(d, "connID"),
                },
                Err(e) => {
                    tracing::warn!(error = %e, "undecodable calls signal payload");
                    CallsEvent::Unhandled {
                        name: name.to_string(),
                        data: d.clone(),
                    }
                }
            }
        }
        ev::CALL_START => CallsEvent::CallStarted {
            call_id: s(d, "id"),
            channel_id,
            start_at: i(d, "start_at"),
            thread_id: s(d, "thread_id"),
            post_id: s(d, "post_id"),
            owner_id: s(d, "owner_id"),
            host_id: s(d, "host_id"),
        },
        ev::CALL_STATE => {
            // `call` arrives as a JSON string, like most model payloads.
            let state = d
                .get("call")
                .and_then(Value::as_str)
                .and_then(|raw| serde_json::from_str::<CallState>(raw).ok())
                .unwrap_or_default();
            CallsEvent::CallState {
                channel_id,
                state: Box::new(state),
            }
        }
        ev::CALL_END => CallsEvent::CallEnded { channel_id },
        ev::USER_JOINED => CallsEvent::UserJoined {
            user_id,
            session_id,
            channel_id,
        },
        ev::USER_LEFT => CallsEvent::UserLeft {
            user_id,
            session_id,
            channel_id,
        },
        ev::USER_MUTED | ev::USER_UNMUTED => CallsEvent::UserMuted {
            user_id,
            session_id,
            muted: name == ev::USER_MUTED,
        },
        ev::USER_VOICE_ON | ev::USER_VOICE_OFF => CallsEvent::UserSpeaking {
            user_id,
            session_id,
            speaking: name == ev::USER_VOICE_ON,
        },
        ev::USER_SCREEN_ON | ev::USER_SCREEN_OFF => CallsEvent::UserScreenShare {
            user_id,
            session_id,
            sharing: name == ev::USER_SCREEN_ON,
        },
        ev::USER_VIDEO_ON | ev::USER_VIDEO_OFF => CallsEvent::UserVideo {
            user_id,
            session_id,
            on: name == ev::USER_VIDEO_ON,
        },
        ev::USER_RAISE_HAND | ev::USER_UNRAISE_HAND => CallsEvent::UserRaisedHand {
            user_id,
            session_id,
            raised_at: if name == ev::USER_RAISE_HAND {
                i(d, "raised_hand")
            } else {
                0
            },
        },
        ev::USER_REACTED => CallsEvent::UserReacted {
            user_id,
            session_id,
            reaction: d
                .get("emoji")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            timestamp: i(d, "timestamp"),
        },
        ev::CALL_HOST_CHANGED => CallsEvent::HostChanged {
            host_id: s(d, "hostID"),
            call_id: s(d, "call_id"),
        },
        ev::CALL_JOB_STATE => CallsEvent::JobState {
            call_id: s(d, "callID"),
            state: d
                .get("jobState")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        },
        ev::HOST_MUTE => CallsEvent::HostMuteRequest { session_id },
        ev::HOST_SCREEN_OFF => CallsEvent::HostScreenOffRequest { session_id },
        ev::HOST_LOWER_HAND => CallsEvent::HostLowerHandRequest { session_id },
        ev::HOST_REMOVED => CallsEvent::HostRemoved {
            session_id,
            user_id,
        },
        _ => CallsEvent::Unhandled {
            name: name.to_string(),
            data: d.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::ws::WsFrame;

    fn ev(json: &str) -> Option<CallsEvent> {
        let frame: WsFrame = serde_json::from_str(json).unwrap();
        parse(&Event::from_frame(&frame))
    }

    #[test]
    fn non_calls_events_are_ignored() {
        assert!(ev(r#"{"event":"posted","data":{},"broadcast":{},"seq":1}"#).is_none());
        assert!(
            ev(r#"{"event":"custom_other.plugin_x","data":{},"broadcast":{},"seq":1}"#).is_none()
        );
    }

    #[test]
    fn join_ack_carries_our_session_id() {
        match ev(r#"{"event":"custom_com.mattermost.calls_join",
                    "data":{"connID":"sess1"},"broadcast":{},"seq":1}"#)
        .unwrap()
        {
            CallsEvent::JoinAccepted { conn_id } => assert_eq!(conn_id, "sess1"),
            other => panic!("expected JoinAccepted, got {other:?}"),
        }
    }

    #[test]
    fn signal_payload_is_an_uncompressed_json_string() {
        let json = r#"{"event":"custom_com.mattermost.calls_signal",
            "data":{"data":"{\"type\":\"answer\",\"sdp\":\"v=0\\r\\n\"}","connID":"s1"},
            "broadcast":{},"seq":2}"#;
        match ev(json).unwrap() {
            CallsEvent::Signal {
                signal: protocol::Signal::Answer { sdp },
                conn_id,
            } => {
                assert!(sdp.starts_with("v=0"));
                assert_eq!(conn_id, "s1");
            }
            other => panic!("expected an answer signal, got {other:?}"),
        }
    }

    #[test]
    fn mute_events_use_the_camel_case_user_key() {
        // This event really does spell it `userID`, unlike `user_joined`.
        match ev(r#"{"event":"custom_com.mattermost.calls_user_muted",
                    "data":{"userID":"u1","session_id":"s1"},"broadcast":{},"seq":3}"#)
        .unwrap()
        {
            CallsEvent::UserMuted {
                user_id,
                session_id,
                muted,
            } => {
                assert_eq!(user_id, "u1");
                assert_eq!(session_id, "s1");
                assert!(muted);
            }
            other => panic!("expected UserMuted, got {other:?}"),
        }
    }

    #[test]
    fn user_joined_uses_the_snake_case_user_key() {
        match ev(r#"{"event":"custom_com.mattermost.calls_user_joined",
                    "data":{"user_id":"u2","session_id":"s2"},
                    "broadcast":{"channel_id":"c1"},"seq":4}"#)
        .unwrap()
        {
            CallsEvent::UserJoined {
                user_id,
                session_id,
                channel_id,
            } => {
                assert_eq!(user_id, "u2");
                assert_eq!(session_id, "s2");
                // Only the broadcast names the channel for this event.
                assert_eq!(channel_id, "c1");
            }
            other => panic!("expected UserJoined, got {other:?}"),
        }
    }

    #[test]
    fn call_state_unwraps_the_double_encoded_roster() {
        let json = r#"{"event":"custom_com.mattermost.calls_call_state",
            "data":{"channel_id":"c1","call":"{\"id\":\"call1\",\"sessions\":[{\"session_id\":\"s1\",\"user_id\":\"u1\",\"unmuted\":true}],\"host_id\":\"u1\"}"},
            "broadcast":{},"seq":5}"#;
        match ev(json).unwrap() {
            CallsEvent::CallState { channel_id, state } => {
                assert_eq!(channel_id, "c1");
                assert_eq!(state.participant_count(), 1);
                assert_eq!(state.user_for_session("s1"), Some("u1"));
            }
            other => panic!("expected CallState, got {other:?}"),
        }
    }

    #[test]
    fn voice_events_map_to_speaking() {
        match ev(r#"{"event":"custom_com.mattermost.calls_user_voice_on",
                    "data":{"userID":"u1","session_id":"s1"},"broadcast":{},"seq":6}"#)
        .unwrap()
        {
            CallsEvent::UserSpeaking { speaking, .. } => assert!(speaking),
            other => panic!("expected UserSpeaking, got {other:?}"),
        }
        match ev(r#"{"event":"custom_com.mattermost.calls_user_voice_off",
                    "data":{"userID":"u1","session_id":"s1"},"broadcast":{},"seq":7}"#)
        .unwrap()
        {
            CallsEvent::UserSpeaking { speaking, .. } => assert!(!speaking),
            other => panic!("expected UserSpeaking, got {other:?}"),
        }
    }

    #[test]
    fn lowering_a_hand_reports_zero() {
        match ev(r#"{"event":"custom_com.mattermost.calls_user_unraise_hand",
                    "data":{"userID":"u1","session_id":"s1","raised_hand":999},
                    "broadcast":{},"seq":8}"#)
        .unwrap()
        {
            CallsEvent::UserRaisedHand { raised_at, .. } => assert_eq!(raised_at, 0),
            other => panic!("expected UserRaisedHand, got {other:?}"),
        }
    }

    #[test]
    fn unknown_calls_events_are_preserved_not_dropped() {
        match ev(r#"{"event":"custom_com.mattermost.calls_caption",
                    "data":{"text":"hello"},"broadcast":{},"seq":9}"#)
        .unwrap()
        {
            CallsEvent::Unhandled { name, data } => {
                assert_eq!(name, "caption");
                assert_eq!(data.get("text").unwrap(), "hello");
            }
            other => panic!("expected Unhandled, got {other:?}"),
        }
    }
}
