//! Bridge between GPUI's main thread and Tokio.
//!
//! Entities and windows are not `Send`, and Tokio futures generally are. The
//! shape that keeps both happy: run the future on a Tokio worker, ship the
//! *result* back over an `async_channel`, and touch the application only in
//! the foreground task that receives it.
//!
//! Those foreground tasks are polled by GPUI's own executor, outside any
//! application update, so the callback is handed a fresh `&mut App` rather
//! than borrowing one that is already lent out.

use std::cell::RefCell;
use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use gpui_kit::{App, AsyncApp};
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

thread_local! {
    /// The application, as seen from a task. GPUI is single-threaded, so a
    /// thread-local is the whole of the storage needed; it is set once, before
    /// the first window is built.
    static APP: RefCell<Option<AsyncApp>> = const { RefCell::new(None) };
}

/// Makes the application reachable from [`spawn`] and friends. Call once, at
/// startup.
pub fn install(cx: &mut App) {
    APP.with_borrow_mut(|app| *app = Some(cx.to_async()));
}

fn app() -> AsyncApp {
    APP.with_borrow(|app| app.clone())
        .expect("runtime::install runs before anything is spawned")
}

/// The shared multi-threaded Tokio runtime.
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            // REST, websocket and cache work is I/O-bound. Tokio's default is
            // one worker per logical CPU (14 on the profiling machine), which
            // bought no latency and reserved a stack per idle worker. Two can
            // overlap network and disk without turning a desktop client into
            // a thread farm; blocking codecs use their own dedicated pools.
            .worker_threads(2)
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
/// calling it straight from a UI callback panics with "there is no reactor
/// running" — which is how the websocket managed to never start at all while
/// every REST call kept working.
pub fn with_runtime<T>(f: impl FnOnce() -> T) -> T {
    let _guard = runtime().enter();
    f()
}

/// Runs `fut` off-thread and calls `on_done` on the main thread with its
/// result.
pub fn spawn<T, F, C>(fut: F, on_done: C)
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
    C: FnOnce(T, &mut App) + 'static,
{
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let _ = tx.send(fut.await).await;
    });
    let app = app();
    app.foreground_executor()
        .spawn({
            let app = app.clone();
            async move {
                if let Ok(value) = rx.recv().await {
                    app.update(|cx| on_done(value, cx));
                }
            }
        })
        .detach();
}

/// Like [`spawn`], but for a producer that yields many values — a websocket
/// subscription, for instance. `on_item` runs on the main thread for each.
///
/// The Tokio side stops as soon as the receiving side is dropped, so a session
/// that has gone away does not leave a task spinning.
pub fn spawn_stream<T, F, Fut, C>(producer: F, mut on_item: C)
where
    F: FnOnce(async_channel::Sender<T>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
    T: Send + 'static,
    C: FnMut(T, &mut App) + 'static,
{
    let (tx, rx) = async_channel::bounded(64);
    runtime().spawn(producer(tx));
    let app = app();
    app.foreground_executor()
        .spawn({
            let app = app.clone();
            async move {
                while let Ok(item) = rx.recv().await {
                    app.update(|cx| on_item(item, cx));
                }
            }
        })
        .detach();
}

/// Calls `f` on the main thread once `delay` has passed.
pub fn after(delay: Duration, f: impl FnOnce(&mut App) + 'static) {
    let app = app();
    let timer = app.background_executor().timer(delay);
    app.foreground_executor()
        .spawn({
            let app = app.clone();
            async move {
                timer.await;
                app.update(f);
            }
        })
        .detach();
}

/// Calls `f` on the main thread as soon as whatever is running now has
/// finished — after the current update, never inside it.
pub fn soon(f: impl FnOnce(&mut App) + 'static) {
    after(Duration::ZERO, f);
}

/// Drains a channel on the main thread, for a producer that is not a Tokio
/// future — a decoder's callback, say. `on_item` answers whether to keep
/// going.
pub fn receive<T: 'static>(
    rx: async_channel::Receiver<T>,
    mut on_item: impl FnMut(T, &mut App) -> bool + 'static,
) {
    let app = app();
    app.foreground_executor()
        .spawn({
            let app = app.clone();
            async move {
                while let Ok(item) = rx.recv().await {
                    if !app.update(|cx| on_item(item, cx)) {
                        break;
                    }
                }
            }
        })
        .detach();
}
