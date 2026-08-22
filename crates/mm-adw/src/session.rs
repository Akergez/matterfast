//! Remembering a session across restarts.
//!
//! Only the server URL and the session token are kept — never the password.
//! The token is what Mattermost itself hands out and can be revoked from
//! Security settings, so losing it is recoverable in a way a password is not.
//!
//! ponytail: plaintext file at 0600, not the GNOME keyring. Same protection as
//! `~/.ssh/id_rsa` or `~/.docker/config.json` — anything running as this user
//! can read it. Move it to the Secret Service (the `oo7` crate speaks it with
//! no C dependency) if the token ever needs to survive an untrusted local
//! process.

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

/// The stored session, if there is one.
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

/// Forgets the stored session — the token was refused, or the user signed out.
pub fn clear() {
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

        clear();
        assert_eq!(load(), None);
        clear(); // clearing twice is not an error

        let _ = fs::remove_dir_all(&dir);
    }
}
