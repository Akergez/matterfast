//! One running copy, and a way for a second launch to reach it.
//!
//! Two things depend on this. Clicking the launcher again must raise the
//! window that is already there rather than start a second session with a
//! second websocket; and the browser finishes a single sign-on by launching
//! this binary again with a `mattermost-dev://…` URI, which only means
//! something to the copy that started the sign-in.
//!
//! The toolkit does not do either on Linux, so it is done here with the
//! oldest tool there is: a socket in the runtime directory. A launch that
//! finds a live socket says what it was started for and exits; a launch that
//! does not becomes the one listening.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

/// What a second launch asks the running one to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Show a window: the launcher was clicked again.
    Activate,
    /// A URI this application is registered to handle.
    Open(String),
}

impl Request {
    fn encode(&self) -> String {
        match self {
            Request::Activate => "activate\n".to_string(),
            // A newline would end the message early; a URI never has one, and
            // one that does is not something to pass on.
            Request::Open(uri) => format!("open {}\n", uri.replace(['\n', '\r'], "")),
        }
    }

    fn decode(line: &str) -> Option<Request> {
        let line = line.trim_end_matches(['\n', '\r']);
        if line == "activate" {
            return Some(Request::Activate);
        }
        line.strip_prefix("open ")
            .filter(|uri| !uri.is_empty())
            .map(|uri| Request::Open(uri.to_string()))
    }
}

/// What this launch was started for: every URI on the command line, or a
/// plain activation when there is none.
pub fn requests_from_args(args: impl Iterator<Item = String>) -> Vec<Request> {
    let opens: Vec<Request> = args
        .filter(|arg| arg.contains("://"))
        .map(Request::Open)
        .collect();
    if opens.is_empty() {
        vec![Request::Activate]
    } else {
        opens
    }
}

/// Where the socket lives.
///
/// Inside a Flatpak the runtime directory is private to each sandbox, and
/// only `app/<id>` under it is shared between two launches of the same app —
/// so that is where it has to go, or the second launch never finds the first.
fn socket_path() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(crate::paths::cache_dir);
    let dir = match std::env::var_os("FLATPAK_ID") {
        Some(id) => runtime.join("app").join(id),
        None => runtime,
    };
    dir.join(format!("{}.sock", crate::APP_ID))
}

/// Hands this launch's requests to a copy that is already running. `true`
/// means one took them and this process has nothing left to do.
pub fn forward(requests: &[Request]) -> bool {
    forward_to(&socket_path(), requests)
}

fn forward_to(path: &std::path::Path, requests: &[Request]) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else {
        return false;
    };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    for request in requests {
        if stream.write_all(request.encode().as_bytes()).is_err() {
            return false;
        }
    }
    true
}

/// Becomes the running copy: listens for later launches and yields what they
/// ask for. `None` when the socket cannot be made, in which case the
/// application still works — it just cannot be reached by a second launch.
pub fn listen() -> Option<async_channel::Receiver<Request>> {
    listen_at(socket_path())
}

fn listen_at(path: PathBuf) -> Option<async_channel::Receiver<Request>> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Nobody answered on it (the caller tried `forward` first), so whatever is
    // at this path was left behind by a copy that is gone.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "could not listen for later launches");
            return None;
        }
    };

    let (tx, rx) = async_channel::unbounded();
    std::thread::Builder::new()
        .name("mm-ipc".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                // A launch that connects and then says nothing must not hold
                // the door for everyone after it.
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    match Request::decode(&line) {
                        Some(request) => {
                            if tx.send_blocking(request).is_err() {
                                return; // the application is gone
                            }
                        }
                        None => tracing::warn!("ignoring a launch request that made no sense"),
                    }
                }
            }
        })
        .ok()?;
    Some(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launch_with_a_uri_asks_for_it_to_be_opened() {
        let args = ["mattermost-dev://h/login/desktop?a=b".to_string()];
        assert_eq!(
            requests_from_args(args.into_iter()),
            vec![Request::Open("mattermost-dev://h/login/desktop?a=b".into())]
        );
        // No URI: it was the launcher, and the window should come up.
        assert_eq!(
            requests_from_args(["--some-flag".to_string()].into_iter()),
            vec![Request::Activate]
        );
        assert_eq!(requests_from_args(std::iter::empty()), vec![Request::Activate]);
    }

    #[test]
    fn requests_survive_the_wire() {
        for request in [
            Request::Activate,
            Request::Open("mattermost-dev://h/login/desktop?client_token=a&server_token=b".into()),
        ] {
            assert_eq!(Request::decode(&request.encode()), Some(request));
        }
        assert_eq!(Request::decode("nonsense"), None);
        assert_eq!(Request::decode("open "), None);
        // One request per line, whatever the URI tried to carry.
        assert_eq!(Request::Open("a://b\nactivate".into()).encode().lines().count(), 1);
    }

    #[test]
    fn a_second_launch_reaches_the_first() {
        let dir = std::env::temp_dir().join(format!("matterfast-ipc-test-{}", std::process::id()));
        let path = dir.join("test.sock");

        // Nobody is listening yet.
        assert!(!forward_to(&path, &[Request::Activate]));

        let rx = listen_at(path.clone()).expect("the socket can be made");
        assert!(forward_to(
            &path,
            &[Request::Open("mattermost-dev://x?y=z".into()), Request::Activate]
        ));
        assert_eq!(
            rx.recv_blocking().unwrap(),
            Request::Open("mattermost-dev://x?y=z".into())
        );
        assert_eq!(rx.recv_blocking().unwrap(), Request::Activate);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
