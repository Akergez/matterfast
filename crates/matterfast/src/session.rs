//! Remembering a session across restarts.
//!
//! Only the server URL and the session token are kept — never the password.
//! The token is what Mattermost itself hands out and can be revoked from
//! Security settings, so losing it is recoverable in a way a password is not.
//!
//! It lives in the system's own secret store — the Secret Service on Linux
//! (GNOME Keyring, KWallet, anything else speaking the D-Bus interface), the
//! Keychain on macOS, the Credential Manager on Windows — which means it is
//! encrypted at rest and another process running as this user cannot simply
//! read it. That is a real
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
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use serde_json::json;

fn path() -> PathBuf {
    crate::paths::config_dir()
        .join(crate::APP_ID)
        .join("session.json")
}

/// The Secret Service: one item per server, found by its attributes.
#[cfg(target_os = "linux")]
mod vault {
    use std::collections::HashMap;

    pub type Error = oo7::Error;

    /// What the keyring item is labelled and keyed by. The attributes are the
    /// lookup key, so they have to be stable across versions.
    const LABEL: &str = "Matterfast session token";

    /// Every session, or — narrowed by the server — one. The server is part
    /// of the key so several accounts can be stored side by side rather than
    /// overwriting each other.
    fn attributes(server: Option<&str>) -> HashMap<&str, &str> {
        let mut attributes =
            HashMap::from([("application", crate::APP_ID), ("type", "session")]);
        if let Some(server) = server {
            attributes.insert("server", server);
        }
        attributes
    }

    pub async fn all() -> Result<Vec<Vec<u8>>, Error> {
        let keyring = oo7::Keyring::new().await?;
        let mut secrets = Vec::new();
        for item in keyring.search_items(&attributes(None)).await? {
            if let Ok(secret) = item.secret().await {
                secrets.push(secret.to_vec());
            }
        }
        Ok(secrets)
    }

    pub async fn put(server: &str, secret: &[u8]) -> Result<(), Error> {
        let keyring = oo7::Keyring::new().await?;
        keyring
            .create_item(LABEL, &attributes(Some(server)), secret, true)
            .await
    }

    /// Removes one server's item, or with `None` all of them.
    pub async fn remove(server: Option<&str>) -> Result<(), Error> {
        let keyring = oo7::Keyring::new().await?;
        keyring.delete(&attributes(server)).await
    }
}

/// The Keychain and the Credential Manager, which cannot be searched the way
/// the Secret Service can: an entry is found by its exact name or not at all.
/// So every session is kept in one entry, as a list.
#[cfg(not(any(target_os = "linux", target_os = "android")))]
mod vault {
    pub type Error = keyring::Error;

    fn entry() -> Result<keyring::Entry, Error> {
        keyring::Entry::new(crate::APP_ID, "sessions")
    }

    /// (server, secret) pairs, oldest first.
    fn read() -> Result<Vec<(String, String)>, Error> {
        match entry()?.get_password() {
            Ok(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
            Err(keyring::Error::NoEntry) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn write(sessions: &[(String, String)]) -> Result<(), Error> {
        if sessions.is_empty() {
            return match entry()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(error),
            };
        }
        let raw = serde_json::to_string(sessions).expect("strings always serialise");
        entry()?.set_password(&raw)
    }

    pub async fn all() -> Result<Vec<Vec<u8>>, Error> {
        Ok(read()?.into_iter().map(|(_, secret)| secret.into_bytes()).collect())
    }

    pub async fn put(server: &str, secret: &[u8]) -> Result<(), Error> {
        let mut sessions = read()?;
        sessions.retain(|(stored, _)| stored != server);
        sessions.push((server.to_string(), String::from_utf8_lossy(secret).into_owned()));
        write(&sessions)
    }

    /// Removes one server's session, or with `None` all of them.
    pub async fn remove(server: Option<&str>) -> Result<(), Error> {
        let mut sessions = read()?;
        match server {
            Some(server) => sessions.retain(|(stored, _)| stored != server),
            None => sessions.clear(),
        }
        write(&sessions)
    }
}

/// Android, where the `keyring` crate has no store and quietly keeps secrets
/// in memory, to be gone at the next launch. The application's own directory
/// is the store there: no other application can read it, which is the
/// property the keyrings are relied on for elsewhere.
#[cfg(target_os = "android")]
mod vault {
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;

    pub type Error = std::io::Error;

    fn path() -> PathBuf {
        crate::paths::config_dir()
            .join(crate::APP_ID)
            .join("sessions.json")
    }

    /// (server, secret) pairs, oldest first.
    fn read() -> Result<Vec<(String, String)>, Error> {
        match fs::read_to_string(path()) {
            Ok(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn write(sessions: &[(String, String)]) -> Result<(), Error> {
        let file = path();
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let raw = serde_json::to_string(sessions).expect("strings always serialise");
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&file)?;
        f.write_all(raw.as_bytes())?;
        f.sync_all()
    }

    pub async fn all() -> Result<Vec<Vec<u8>>, Error> {
        Ok(read()?.into_iter().map(|(_, secret)| secret.into_bytes()).collect())
    }

    pub async fn put(server: &str, secret: &[u8]) -> Result<(), Error> {
        let mut sessions = read()?;
        sessions.retain(|(stored, _)| stored != server);
        sessions.push((server.to_string(), String::from_utf8_lossy(secret).into_owned()));
        write(&sessions)
    }

    /// Removes one server's session, or with `None` all of them.
    pub async fn remove(server: Option<&str>) -> Result<(), Error> {
        let mut sessions = read()?;
        match server {
            Some(server) => sessions.retain(|(stored, _)| stored != server),
            None => sessions.clear(),
        }
        write(&sessions)
    }
}

/// Every stored session, newest last. Used to offer a choice when more than
/// one server is signed in.
pub async fn load_all_async() -> Vec<(String, String)> {
    let found = vault::all().await.map(|secrets| {
        let mut sessions = Vec::new();
        for secret in secrets {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&secret) {
                let read = |key: &str| {
                    value
                        .get(key)
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                        .filter(|s| !s.is_empty())
                };
                if let (Some(server), Some(token)) = (read("server"), read("token")) {
                    sessions.push((server, token));
                }
            }
        }
        sessions
    });

    match found {
        Ok(sessions) if !sessions.is_empty() => sessions,
        // Nothing in the keyring: either a first run, or an upgrade from the
        // version that wrote a file. Migrate it in and take it out of
        // circulation, so the fallback is read at most once.
        Ok(_) => match load() {
            Some((server, token)) => {
                save_async(&server, &token).await;
                clear_file();
                vec![(server, token)]
            }
            None => Vec::new(),
        },
        Err(e) => {
            tracing::warn!("keyring unavailable, using the file: {e}");
            load().into_iter().collect()
        }
    }
}

/// Forgets one server's session, leaving any others alone.
pub async fn forget_async(server: &str) {
    if let Err(e) = vault::remove(Some(server)).await {
        tracing::debug!("could not forget {server}: {e}");
    }
}

/// Stores the session in the keyring, replacing whatever was there.
pub async fn save_async(server: &str, token: &str) {
    let secret = json!({ "server": server, "token": token }).to_string();
    if let Err(e) = vault::put(server, secret.as_bytes()).await {
        tracing::warn!("could not reach the keyring, falling back to a file: {e}");
        save(server, token);
    }
}

/// Forgets the session in both places. Signing out has to clear the fallback
/// too, or the next launch signs straight back in with it.
pub async fn clear_async() {
    if let Err(e) = vault::remove(None).await {
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
    #[cfg(unix)]
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;

    // Created 0600 up front rather than chmod-ed afterwards: the gap between
    // the two is long enough for another process to open the file.
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut f = options.open(&file)?;
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
        let dir = std::env::temp_dir().join(format!("matterfast-session-test-{}", std::process::id()));
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
