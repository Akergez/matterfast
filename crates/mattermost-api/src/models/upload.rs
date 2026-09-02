//! Resumable upload sessions (`server/public/model/upload_session.go`).

use serde::{Deserialize, Serialize};

/// One in-progress upload. Created up front with the *total* size, then filled
/// by POSTing bytes; `file_offset` is how many the server already holds and is
/// the only thing that says where to resume from.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct UploadSession {
    pub id: String,
    /// `"attachment"` for a channel upload, `"import"` for an admin import.
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub file_size: i64,
    #[serde(default)]
    pub file_offset: i64,
}

impl UploadSession {
    /// Bytes still owed to the server.
    pub fn remaining(&self) -> i64 {
        (self.file_size - self.file_offset).max(0)
    }

    pub fn is_complete(&self) -> bool {
        self.file_offset >= self.file_size
    }
}
