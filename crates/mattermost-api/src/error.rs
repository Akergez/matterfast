//! Error types.
//!
//! Every non-2xx response from Mattermost carries an [`AppError`] JSON body
//! (`server/public/model/utils.go`), so we surface it verbatim rather than
//! collapsing it into a string — callers routinely branch on `id`.

use serde::Deserialize;

/// The JSON body Mattermost returns for any non-2xx response.
///
/// ```json
/// {
///   "id": "api.user.login.invalid_credentials_email_username",
///   "message": "Enter a valid email or username and/or password.",
///   "detailed_error": "",
///   "request_id": "…",
///   "status_code": 401
/// }
/// ```
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AppError {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub detailed_error: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub status_code: u16,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            write!(f, "{} ({})", self.id, self.status_code)
        } else {
            write!(f, "{} [{}]", self.message, self.id)
        }
    }
}

impl AppError {
    /// True when the server rejected the session — the caller should re-login.
    pub fn is_session_expired(&self) -> bool {
        matches!(
            self.id.as_str(),
            "api.context.session_expired.app_error" | "api.context.invalid_token.error"
        ) || self.status_code == 401
    }

    /// True when MFA is required but was not supplied.
    pub fn is_mfa_required(&self) -> bool {
        self.id.starts_with("mfa.")
            || self.id == "api.user.check_user_mfa.bad_code.app_error"
            || self.id == "api.context.mfa_required.app_error"
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A structured error from the Mattermost API.
    #[error("mattermost: {0}")]
    Api(#[from] AppError),

    /// Transport-level failure (DNS, TLS, connect, timeout).
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),

    /// The response body did not deserialize into the expected shape.
    #[error("decoding {context}: {source}")]
    Decode {
        context: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error("websocket: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),

    /// `POST /users/login` succeeded but the `Token` response header was absent.
    #[error("login succeeded but the server did not return a Token header")]
    MissingToken,

    /// The websocket closed and the client is not going to reconnect.
    #[error("websocket closed: {0}")]
    Closed(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// True when this failed because the server's certificate was refused,
    /// rather than because the server could not be reached at all — the two
    /// arrive as the same reqwest "connect" error but call for opposite fixes.
    /// reqwest exposes no predicate for it, so the source chain is all we have.
    pub fn is_cert_failure(&self) -> bool {
        let Error::Http(e) = self else { return false };
        if !e.is_connect() {
            return false;
        }
        let mut source: Option<&dyn std::error::Error> = std::error::Error::source(e);
        while let Some(e) = source {
            if e.to_string().contains("invalid peer certificate") {
                return true;
            }
            source = e.source();
        }
        false
    }
}

impl std::error::Error for AppError {}

pub type Result<T> = std::result::Result<T, Error>;
