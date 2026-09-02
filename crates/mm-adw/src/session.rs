//! Remembering a session across restarts.
//!
//! Only the server URL and the session token are kept — never the password.
//! The token is what Mattermost itself hands out and can be revoked from
//! Security settings, so losing it is recoverable in a way a password is not.
//!
//! It lives in the Secret Service (GNOME Keyring, KWallet, anything else
//! speaking the D-Bus interface), which means it is encrypted at rest and
//! another process running as this user cannot simply read it. That is a real
//! difference for a chat token: it grants your whole account for as long as
//! nobody revokes it.
//!
//! There is a plaintext fallback at `~/.config/<app>/session.json`, for
//! machines with no Secret Service running at all — a headless box, a minimal
//! session. Falling back is announced in the log, because the difference
//! matters and silently downgrading a security property is how it stops being
//! one. The fallback is also what older versions wrote, so it is read once and
//! migrated.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use gtk::glib;
use serde_json::json;

fn path() -> PathBuf {
    glib::user_config_dir()
        .join(crate::APP_ID)
        .join("session.json")
}

/// What the keyring item is labelled and keyed by. The attributes are the
/// lookup key, so they have to be stable across versions.
const LABEL: &str = "Mattermost session token";

fn attributes() -> std::collections::HashMap<&'static str, &'static str> {
    std::collections::HashMap::from([("application", crate::APP_ID), ("type", "session")])
}

/// Reads the session out of the keyring, falling back to the old plaintext
/// file — and migrating it in, so the fallback is used at most once.
///
/// Async because the Secret Service is a D-Bus service that may need to prompt
/// the user to unlock. Blocking the GTK thread on that would freeze the window
/// behind the unlock dialog.
pub async fn load_async() -> Option<(String, String)> {
    match keyring_load().await {
        Ok(Some(found)) => return Some(found),
        Ok(None) => {}
        Err(e) => tracing::warn!("keyring unavailable, using the file: {e}"),
    }
    // Either nothing is stored, or this is an upgrade from a version that
    // wrote the file. Move it in and take the file back out of circulation.
    let found = load()?;
    save_async(&found.0, &found.1).await;
    clear_file();
    Some(found)
}

async fn keyring_load() -> Result<Option<(String, String)>, oo7::Error> {
    let keyring = oo7::Keyring::new().await?;
    let items = keyring.search_items(&attributes()).await?;
    let Some(item) = items.first() else {
        return Ok(None);
    };
    let secret = item.secret().await?;
    let stored: serde_json::Value = match serde_json::from_slice(&secret) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let read = |key: &str| stored.get(key).and_then(|v| v.as_str()).map(str::to_owned);
    Ok(match (read("server"), read("token")) {
        (Some(server), Some(token)) if !server.is_empty() && !token.is_empty() => {
            Some((server, token))
        }
        _ => None,
    })
}

/// Stores the session in the keyring, replacing whatever was there.
pub async fn save_async(server: &str, token: &str) {
    let secret = json!({ "server": server, "token": token }).to_string();
    let result = async {
        let keyring = oo7::Keyring::new().await?;
        keyring
            .create_item(LABEL, &attributes(), secret.as_bytes(), true)
            .await
    }
    .await;
    if let Err(e) = result {
        tracing::warn!("could not reach the keyring, falling back to a file: {e}");
        save(server, token);
    }
}

/// Forgets the session in both places. Signing out has to clear the fallback
/// too, or the next launch signs straight back in with it.
pub async fn clear_async() {
    let result = async {
        let keyring = oo7::Keyring::new().await?;
        keyring.delete(&attributes()).await
    }
    .await;
    if let Err(e) = result {
        tracing::debug!("could not clear the keyring item: {e}");
    }
    clear_file();
}

/// The stored session from the plaintext fallback, if there is one.
pub fn load() -> Option<(String, String)> {
    let raw = fs::read_to_string(path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let server = v.get("server")?.as_str()?.to_string();
    let token = v.get("token")?.as_str()?.to_string();
    if server.is_empty() || token.is_empty() {
        return None;
    }
    Some((server, token))
}

/// Replaces the stored session. Failures are logged, never fatal: not being
/// able to write the file is a reason to sign in again next launch, not a
/// reason to interrupt a session that is otherwise working.
pub fn save(server: &str, token: &str) {
    if let Err(e) = write(server, token) {
        tracing::warn!("could not save the session: {e}");
    }
}

fn write(server: &str, token: &str) -> std::io::Result<()> {
    let file = path();
    let dir = file.parent().expect("session path always has a parent");
    fs::create_dir_all(dir)?;
    // The token would otherwise sit in a directory anyone can list.
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;

    // Created 0600 up front rather than chmod-ed afterwards: the gap between
    // the two is long enough for another process to open the file.
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&file)?;
    f.write_all(
        json!({ "server": server, "token": token })
            .to_string()
            .as_bytes(),
    )?;
    f.sync_all()
}

/// Forgets the plaintext fallback.
pub fn clear_file() {
    match fs::remove_file(path()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("could not clear the session: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_not_readable_by_anyone_else() {
        // `path()` follows XDG, so point it somewhere disposable.
        let dir = std::env::temp_dir().join(format!("mm-adw-session-test-{}", std::process::id()));
        // SAFETY: single-threaded test, before any other thread reads the env.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };

        assert_eq!(load(), None, "nothing stored yet");

        save("https://mm.example.com", "tok-1");
        assert_eq!(
            load(),
            Some(("https://mm.example.com".into(), "tok-1".into()))
        );

        let mode = fs::metadata(path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the token must not be world-readable");

        save("https://mm.example.com", "tok-2");
        assert_eq!(load().unwrap().1, "tok-2", "save replaces, never appends");

        clear_file();
        assert_eq!(load(), None);
        clear_file(); // clearing twice is not an error

        let _ = fs::remove_dir_all(&dir);
    }
}
