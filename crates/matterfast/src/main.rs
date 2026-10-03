//! A native Mattermost client.
//!
//! The window is a three-pane layout — channel sidebar, conversation, and a
//! thread/inbox panel — drawn with GPUI. See [`ui`] for the session and the
//! panes, and [`ipc`] for how a second launch finds the first.

// Without this a release build on Windows opens a console window behind the
// real one. Debug builds keep the console: it is where the log goes.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agents;
mod appearance;
mod audio;
mod avatars;
mod background;
mod cache;
mod demo;
mod emoji;
mod fonts;
mod ipc;
mod markdown;
mod notifications;
mod paths;
#[cfg_attr(not(target_os = "linux"), path = "playback_stub.rs")]
mod playback;
mod resource_cache;
mod runtime;
mod session;
mod state;
mod store;
mod themes;
mod timefmt;
mod ui;
#[cfg(any(windows, test))]
mod url_scheme;
mod video;
mod zed_extensions;
mod zed_theme;

pub const APP_ID: &str = "io.gitlab.akergez.Matterfast";

/// A file for the log, where there is no console to print it on: a release
/// build on Windows is a GUI program, and what it writes to its standard
/// output goes nowhere — so a sign-in that fails there fails without a word.
///
/// Appended to, because the second launch a sign-in link causes writes here
/// too and must not wipe what the first copy said; started over once it has
/// grown, so that it does not grow for ever.
fn log_file() -> Option<std::fs::File> {
    if !cfg!(all(windows, not(debug_assertions))) {
        return None;
    }
    let dir = paths::cache_dir().join(APP_ID);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("matterfast.log");
    if std::fs::metadata(&path).is_ok_and(|file| file.len() > 1 << 20) {
        let _ = std::fs::remove_file(&path);
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

fn main() {
    let log = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "matterfast=debug,mattermost_api=debug,mattermost_calls=debug".into()
        }),
    );
    match log_file() {
        Some(file) => log.with_ansi(false).with_writer(std::sync::Mutex::new(file)).init(),
        None => log.init(),
    }

    // Before anything can reach rustls. Two providers are in the tree —
    // aws-lc-rs through reqwest, ring through webrtc's DTLS — so rustls
    // refuses to pick one, and the thread that first needs TLS panics rather
    // than the process failing visibly.
    mattermost_api::tls::install_crypto_provider();

    // What this launch is for: a `mattermost-dev://` URI is how the browser
    // finishes a single sign-on, and no arguments is the launcher being
    // clicked.
    let requests = ipc::requests_from_args(std::env::args().skip(1));

    // One copy at a time: a second launch hands its request to the first and
    // leaves. Profiling and screenshot runs need an isolated second process
    // without stealing activation from the person's live client, so that is
    // opt-in — normal launches keep the single-instance contract.
    let unique = std::env::var_os("MATTERFAST_NON_UNIQUE").is_none()
        && std::env::var_os("MATTERFAST_DEMO").is_none();
    if unique && ipc::forward(&requests) {
        tracing::info!(requests = requests.len(), "handed this launch to the running copy");
        return;
    }
    tracing::info!(requests = requests.len(), unique, "no running copy took this launch");
    let later_launches = unique.then(ipc::listen).flatten();
    // Only the copy people actually use claims the sign-in links: a demo or
    // a profiling run must not take them away from it.
    #[cfg(windows)]
    if unique {
        url_scheme::register();
    }

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        // Closing the window keeps the process, the socket and the
        // notifications alive — a chat client you have to keep a window open
        // for is not one. Quitting is therefore always explicit.
        .with_quit_mode(gpui_kit::QuitMode::Explicit)
        .run(move |cx| {
            gpui_kit::init(cx);
            fonts::install(cx);
            themes::load(cx);
            appearance::apply(None, cx);
            runtime::install(cx);
            ui::init(cx);

            for request in requests {
                ui::handle_request(request, cx);
            }
            if let Some(later_launches) = later_launches {
                runtime::receive(later_launches, |request, cx| {
                    ui::handle_request(request, cx);
                    true
                });
            }
        });
}
