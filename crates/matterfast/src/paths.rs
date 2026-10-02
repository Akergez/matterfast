//! Where this user's files live.
//!
//! The XDG base directories, resolved once. They used to come from GLib, which
//! reads the environment on first use and never again; sessions, the snapshot
//! and the message store were all written under those answers, so these have
//! to be the same directories byte for byte or an upgrade loses every one of
//! them.

use std::path::PathBuf;
use std::sync::OnceLock;

/// `$XDG_CONFIG_HOME`, or `~/.config`.
pub fn config_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| resolve("XDG_CONFIG_HOME", ".config")).clone()
}

/// `$XDG_CACHE_HOME`, or `~/.cache`.
pub fn cache_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| resolve("XDG_CACHE_HOME", ".cache")).clone()
}

/// `$XDG_DATA_HOME`, or `~/.local/share`.
pub fn data_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| resolve("XDG_DATA_HOME", ".local/share")).clone()
}

fn resolve(variable: &str, fallback: &str) -> PathBuf {
    resolve_from(
        std::env::var_os(variable).map(PathBuf::from),
        dirs::home_dir(),
        fallback,
    )
}

/// The rule itself, apart from the environment so it can be tested: a set,
/// non-empty variable wins, and anything else falls back under the home
/// directory. GLib takes the variable as it is given, so this does too.
fn resolve_from(variable: Option<PathBuf>, home: Option<PathBuf>, fallback: &str) -> PathBuf {
    match variable {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => home.unwrap_or_else(|| PathBuf::from("/")).join(fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_variable_and_falls_back_under_home() {
        let home = Some(PathBuf::from("/home/someone"));
        assert_eq!(
            resolve_from(Some("/elsewhere/config".into()), home.clone(), ".config"),
            PathBuf::from("/elsewhere/config"),
        );
        // Unset and empty mean the same thing, as the XDG spec says.
        assert_eq!(
            resolve_from(None, home.clone(), ".config"),
            PathBuf::from("/home/someone/.config"),
        );
        assert_eq!(
            resolve_from(Some(PathBuf::new()), home.clone(), ".cache"),
            PathBuf::from("/home/someone/.cache"),
        );
        assert_eq!(
            resolve_from(None, home, ".local/share"),
            PathBuf::from("/home/someone/.local/share"),
        );
    }
}
