#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a Zed theme: there is no list of themes in it")]
    NoThemes,
}
