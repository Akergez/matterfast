use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mattermost_calls::CallSession;

/// A joined call and the audio devices serving it.
pub struct ActiveCall {
    pub session: Arc<CallSession>,
    pub channel_id: String,
    pub recording: bool,
    /// Media session id → user id. The media plane carries no user ids, so
    /// naming a track means going through the roster.
    pub roster: HashMap<String, String>,
    /// Dropped with the call, which stops the microphone and speakers.
    pub audio: crate::audio::AudioIo,
    /// Outbound video, if we are sending any. Dropping either stops it.
    pub screen: Option<crate::video::VideoSender>,
    pub camera: Option<crate::video::VideoSender>,
    pub muted: bool,
    /// Who the SFU last reported as speaking, newest first. Server-side voice
    /// activity, so it only ever names other people — we are never in here.
    pub speaking: Vec<String>,
    /// Who is sharing a screen right now, by user id.
    pub sharing: Vec<String>,
    /// Who has their microphone off. The SFU reports mute per session, and
    /// silence is the default, so absence from this set means unmuted.
    pub muted_users: HashSet<String>,
    /// Raised hands, oldest first — the order is the queue.
    pub hands: Vec<String>,
    /// Whose call this is. The host can mute people, end the call and hand the
    /// role on; everyone else sees none of those controls.
    pub host_id: String,
    /// Media session ids by user, needed because host controls address a
    /// *session* rather than a person.
    pub sessions: HashMap<String, String>,
}
