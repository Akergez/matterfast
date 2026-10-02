//! The running copy's address on Windows: a named pipe.
//!
//! Windows has no socket in a runtime directory, but it has the same thing
//! under another name. The pipe is named after the application and the
//! person, because pipe names are shared by everybody signed in to the
//! machine and one person's launch must not land in another's session. The
//! default security of a pipe already lets only its creator write to it.
//!
//! Creating the first instance of a name fails if somebody else has it, which
//! is the whole of the single-instance check: there is nothing to clean up
//! after a copy that died, because its pipe died with it.
//!
//! The pipe is served on the Tokio runtime rather than a thread of its own —
//! the standard library has no named pipe server, and Tokio's is the one that
//! exists.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};

use super::Request;

/// What opening a pipe answers when every instance of it is taken: the copy
/// that is running is busy with another launch, not gone.
const ERROR_PIPE_BUSY: i32 = 231;
/// More than any launch has to say. What arrives is from outside the process.
const LONGEST: u64 = 64 * 1024;
const PATIENCE: Duration = Duration::from_secs(2);

pub fn pipe_name() -> String {
    name_for(crate::APP_ID, &std::env::var("USERNAME").unwrap_or_default())
}

/// A pipe name may not have a backslash in it, and an account name may have
/// nearly anything.
fn name_for(app: &str, user: &str) -> String {
    let user: String = user
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!(r"\\.\pipe\{app}-{user}")
}

pub fn forward_to(name: &str, requests: &[Request]) -> bool {
    let message: String = requests.iter().map(Request::encode).collect();
    crate::runtime::runtime().block_on(async {
        let mut tries = 0;
        let mut pipe = loop {
            match ClientOptions::new().open(name) {
                Ok(pipe) => break pipe,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && tries < 40 => {
                    tries += 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                // No such pipe: nobody is running.
                Err(_) => return false,
            }
        };
        matches!(
            tokio::time::timeout(PATIENCE, pipe.write_all(message.as_bytes())).await,
            Ok(Ok(()))
        )
    })
}

pub fn listen_at(name: String) -> Option<async_channel::Receiver<Request>> {
    let runtime = crate::runtime::runtime();
    // A pipe registers with the reactor of the runtime it is made in.
    let _reactor = runtime.enter();
    let first = match ServerOptions::new().first_pipe_instance(true).create(&name) {
        Ok(first) => first,
        Err(error) => {
            tracing::warn!(%error, name, "could not listen for later launches");
            return None;
        }
    };
    let (tx, rx) = async_channel::unbounded();
    runtime.spawn(serve(first, name, tx));
    Some(rx)
}

async fn serve(mut server: NamedPipeServer, name: String, tx: async_channel::Sender<Request>) {
    loop {
        let connected = server.connect().await;
        // The next instance is made before this one is read, so there is no
        // moment at which a launch finds no pipe and takes itself for the
        // first.
        let next = match ServerOptions::new().create(&name) {
            Ok(next) => next,
            Err(error) => {
                tracing::warn!(%error, "stopped listening for later launches");
                return;
            }
        };
        let pipe = std::mem::replace(&mut server, next);
        if connected.is_ok() {
            tokio::spawn(receive(pipe, tx.clone()));
        }
    }
}

/// One launch's message. The other end closing the pipe is how it ends, and
/// Windows reports that as an error rather than as the end of the data, so
/// whatever was read before it is what counts.
async fn receive(pipe: NamedPipeServer, tx: async_channel::Sender<Request>) {
    let mut bytes = Vec::new();
    let _ = tokio::time::timeout(PATIENCE, pipe.take(LONGEST).read_to_end(&mut bytes)).await;
    for line in String::from_utf8_lossy(&bytes).lines() {
        if let Some(request) = Request::decode(line) {
            if tx.send(request).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_name_is_one_path_segment_whatever_the_account_is_called() {
        assert_eq!(name_for("app.Id", "anton"), r"\\.\pipe\app.Id-anton");
        assert_eq!(name_for("app", r"DOMAIN\some one"), r"\\.\pipe\app-DOMAIN_some_one");
    }

    #[test]
    fn a_second_launch_reaches_the_first_through_the_pipe() {
        let name = name_for("matterfast-ipc-test", &std::process::id().to_string());

        // Nobody is listening yet.
        assert!(!forward_to(&name, &[Request::Activate]));

        let rx = listen_at(name.clone()).expect("the pipe can be made");
        // The name is taken now: a second copy cannot become the listener.
        assert!(listen_at(name.clone()).is_none());

        assert!(forward_to(
            &name,
            &[Request::Open("mattermost-dev://x?y=z".into()), Request::Activate]
        ));
        assert_eq!(
            rx.recv_blocking().unwrap(),
            Request::Open("mattermost-dev://x?y=z".into())
        );
        assert_eq!(rx.recv_blocking().unwrap(), Request::Activate);

        // And again: the pipe outlives one launch.
        assert!(forward_to(&name, &[Request::Activate]));
        assert_eq!(rx.recv_blocking().unwrap(), Request::Activate);
    }
}
