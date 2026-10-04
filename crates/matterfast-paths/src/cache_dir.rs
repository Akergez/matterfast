use std::path::PathBuf;
use std::sync::OnceLock;

use crate::resolve::resolve;

/// `$XDG_CACHE_HOME`, or `~/.cache`.
pub fn cache_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| resolve("XDG_CACHE_HOME", ".cache")).clone()
}
