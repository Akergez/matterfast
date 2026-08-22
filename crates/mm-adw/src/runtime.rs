//! Bridge between GTK's main loop and Tokio.
//!
//! GTK widgets are not `Send`, and Tokio futures generally are. The shape that
//! keeps both happy: run the future on a Tokio worker, ship the *result* back
//! over an `async_channel`, and touch widgets only in the GLib future that
//! receives it.

use std::future::Future;
use std::sync::OnceLock;

use gtk::glib;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The shared multi-threaded Tokio runtime.
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("mm-io")
            .build()
            .expect("failed to start the Tokio runtime")
    })
}

/// Runs `f` with the shared runtime installed as the ambient one.
///
/// Anything that calls `tokio::spawn` internally must be constructed inside
/// this guard. [`mattermost_api::WebSocket::connect`] does exactly that, and
/// calling it straight from a GTK callback panics with "there is no reactor
/// running" — which is how the websocket managed to never start at all while
/// every REST call kept working.
pub fn with_runtime<T>(f: impl FnOnce() -> T) -> T {
    let _guard = runtime().enter();
    f()
}

/// Runs `fut` off-thread and calls `on_done` on the GTK main thread with its
/// result.
pub fn spawn<T, F, C>(fut: F, on_done: C)
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
    C: FnOnce(T) + 'static,
{
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let _ = tx.send(fut.await).await;
    });
    glib::spawn_future_local(async move {
        if let Ok(value) = rx.recv().await {
            on_done(value);
        }
    });
}

/// Like [`spawn`], but for a producer that yields many values — a websocket
/// subscription, for instance. `on_item` runs on the GTK main thread for each.
///
/// The Tokio side stops as soon as the GLib side is dropped, so a closed window
/// does not leave a task spinning.
pub fn spawn_stream<T, F, Fut, C>(producer: F, mut on_item: C)
where
    F: FnOnce(async_channel::Sender<T>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
    T: Send + 'static,
    C: FnMut(T) + 'static,
{
    let (tx, rx) = async_channel::bounded(64);
    runtime().spawn(producer(tx));
    glib::spawn_future_local(async move {
        while let Ok(item) = rx.recv().await {
            on_item(item);
        }
    });
}
