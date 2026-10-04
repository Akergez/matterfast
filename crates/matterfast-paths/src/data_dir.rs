use std::path::PathBuf;
use std::sync::OnceLock;

use crate::resolve::resolve;

/// `$XDG_DATA_HOME`, or `~/.local/share`.
pub fn data_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| resolve("XDG_DATA_HOME", ".local/share")).clone()
}
