//! Errors for the Calls client.

#[derive(Debug, thiserror::Error)]
pub enum CallsError {
    #[error("mattermost: {0}")]
    Api(#[from] mattermost_api::Error),

    #[error("webrtc: {0}")]
    WebRtc(#[from] webrtc::Error),

    #[error("msgpack encode: {0}")]
    MsgPackEncode(#[from] rmp_serde::encode::Error),

    #[error("msgpack decode: {0}")]
    MsgPackDecode(#[from] rmp_serde::decode::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// The server said something the protocol does not allow.
    #[error("protocol: {0}")]
    Protocol(String),

    /// The plugin sent `custom_com.mattermost.calls_error`.
    #[error("server rejected the call: {0}")]
    Rejected(String),

    /// Calls is not enabled, or not enabled for this channel.
    #[error("calls are not available: {0}")]
    Unavailable(String),

    #[error("timed out waiting for {0}")]
    Timeout(&'static str),

    /// The websocket went away mid-call.
    #[error("connection lost: {0}")]
    Disconnected(String),
}

pub type Result<T> = std::result::Result<T, CallsError>;
