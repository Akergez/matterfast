use std::path::PathBuf;
use std::sync::OnceLock;

use crate::resolve::resolve;

/// `$XDG_CONFIG_HOME`, or `~/.config`.
pub fn config_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| resolve("XDG_CONFIG_HOME", ".config")).clone()
}
