//! File metadata (`server/public/model/file_info.go`).

use serde::{Deserialize, Serialize};

use super::Millis;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FileInfo {
    pub id: String,
    /// The Go field is `CreatorId`; the JSON key is `user_id`.
    #[serde(default, rename = "user_id")]
    pub creator_id: String,
    #[serde(default)]
    pub post_id: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub create_at: Millis,
    #[serde(default)]
    pub update_at: Millis,
    #[serde(default)]
    pub delete_at: Millis,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub extension: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub has_preview_image: bool,
    /// Base64 JPEG blob for images; present-but-null otherwise.
    #[serde(default)]
    pub mini_preview: Option<String>,
    #[serde(default)]
    pub archived: bool,
}

impl FileInfo {
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }

    pub fn is_video(&self) -> bool {
        self.mime_type.starts_with("video/")
    }

    /// Human-readable size, e.g. `"1.4 MB"`.
    pub fn human_size(&self) -> String {
        const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
        let mut size = self.size as f64;
        let mut unit = 0;
        while size >= 1024.0 && unit < UNITS.len() - 1 {
            size /= 1024.0;
            unit += 1;
        }
        if unit == 0 {
            format!("{} {}", self.size, UNITS[0])
        } else {
            format!("{size:.1} {}", UNITS[unit])
        }
    }
}

/// `POST /api/v4/files` response.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FileUploadResponse {
    #[serde(default)]
    pub file_infos: Vec<FileInfo>,
    #[serde(default)]
    pub client_ids: Vec<String>,
}
