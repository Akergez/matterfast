//! Staying alive with no window open.
//!
//! A chat client that stops receiving messages the moment its window is closed
//! is a chat client you have to keep a window open for. Every other one on the
//! desktop keeps running, so this one does too: closing the last window leaves
//! the process — and with it the websocket and the notifications — going until
//! something explicitly quits.
//!
//! Whether that is wanted is one boolean, stored next to the session in
//! `~/.config/<app>/settings.json`. It defaults to on, because that is the
//! behaviour people expect from a chat client; failing to read or write it is
//! never worth more than a log line.

use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_CACHE_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// The "keep running with no window" setting.
///
/// Staying alive is the application's own business now — it runs with an
/// explicit quit, so closing the last window ends nothing by itself — and what
/// is left here is the one boolean that says whether that is wanted.
pub struct Background;

impl Background {
    /// Whether running in the background is wanted at all. Defaults to true.
    pub fn enabled() -> bool {
        read(&path()).unwrap_or(true)
    }

    pub fn set_enabled(on: bool) {
        if let Err(e) = write(&path(), on) {
            tracing::warn!("could not save the background setting: {e}");
        }
    }
}

/// Total on-disk HTTP/media cache allowance. Five GiB is large enough for
/// image-heavy channels while still being an explicit, user-visible bound.
pub fn cache_limit_bytes() -> u64 {
    read_value(&path(), "cache_limit_bytes")
        .and_then(|value| value.as_u64())
        .unwrap_or(DEFAULT_CACHE_BYTES)
}

pub fn set_cache_limit_bytes(bytes: u64) {
    if let Err(error) = write_value(&path(), "cache_limit_bytes", bytes.into()) {
        tracing::warn!(%error, "could not save the cache limit");
    }
}

/// Any other key of the shared settings file: nothing when it was never set.
pub fn setting(key: &str) -> Option<serde_json::Value> {
    read_value(&path(), key)
}

pub fn set_setting(key: &str, value: serde_json::Value) {
    if let Err(error) = write_value(&path(), key, value) {
        tracing::warn!(%error, key, "could not save a setting");
    }
}

fn path() -> PathBuf {
    crate::paths::config_dir()
        .join(crate::APP_ID)
        .join("settings.json")
}

fn read(file: &Path) -> Option<bool> {
    read_value(file, "background")?.as_bool()
}

fn read_value(file: &Path, key: &str) -> Option<serde_json::Value> {
    let raw = match fs::read_to_string(file) {
        Ok(raw) => raw,
        // No file is the ordinary first run, not something to report.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!("could not read the settings: {e}");
            return None;
        }
    };
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(v) => v.get(key).cloned(),
        Err(e) => {
            tracing::warn!("could not parse the settings: {e}");
            None
        }
    }
}

/// Rewrites the file with `background` set, leaving any other key alone: this
/// is the shared settings file, and clobbering a key some later version added
/// is the kind of bug nobody traces back to here.
fn write(file: &Path, on: bool) -> std::io::Result<()> {
    write_value(file, "background", on.into())
}

fn write_value(file: &Path, key: &str, value: serde_json::Value) -> std::io::Result<()> {
    fs::create_dir_all(file.parent().expect("settings path always has a parent"))?;
    let mut settings = fs::read_to_string(file)
        .ok()
        .and_then(|raw| {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&raw).ok()
        })
        .unwrap_or_default();
    settings.insert(key.into(), value);
    fs::write(file, serde_json::Value::Object(settings).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults_to_on() {
        let dir =
            std::env::temp_dir().join(format!("matterfast-background-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        // `path()` follows XDG, so point it somewhere disposable.
        // SAFETY: single-threaded test, before any other thread reads the env.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
        assert!(Background::enabled(), "on unless it was turned off");

        // The round trip itself goes through an explicit path rather than
        // `path()`: the user config dir is resolved once, on first use, and the
        // session test sets the same variable, so the two would otherwise
        // share — and delete — one directory.
        let file = dir.join("settings.json");
        assert_eq!(read(&file), None, "nothing stored yet");

        write(&file, false).unwrap();
        assert_eq!(read(&file), Some(false));

        write(&file, true).unwrap();
        assert_eq!(read(&file), Some(true), "writing replaces, never appends");

        // A key this version knows nothing about has to survive a write.
        fs::write(&file, r#"{"background":true,"future":42}"#).unwrap();
        write(&file, false).unwrap();
        let stored: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(stored["background"], serde_json::json!(false));
        assert_eq!(stored["future"], serde_json::json!(42));

        fs::write(&file, "not json at all").unwrap();
        assert_eq!(read(&file), None, "garbage reads as unset, never a panic");

        let _ = fs::remove_dir_all(&dir);
    }
}
