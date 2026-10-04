#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("store: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("store: a cached row is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("store: {0}")]
    Io(#[from] std::io::Error),
    #[error("store: no usable host in the server URL {0:?}")]
    BadServer(String),
}
