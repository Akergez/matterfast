//! A native Mattermost client.
//!
//! The window is a three-pane layout — channel sidebar, conversation, and a
//! thread/inbox panel — drawn with GPUI. See [`ui`] for the session and the
//! panes, and [`ipc`] for how a second launch finds the first.
//!
//! This is a library so that Android can load it: there the system starts a
//! Java activity, which loads this as a shared object and calls
//! [`android_main`]. Everywhere else `main.rs` calls [`run`].

mod agents;
mod appearance;
mod audio;
mod avatars;
mod background;
mod cache;
mod demo;
pub(crate) use matterfast_emoji as emoji;
mod fonts;
// Android keeps one activity itself, so only the requests are used there.
#[cfg_attr(target_os = "android", allow(dead_code))]
mod ipc;
pub(crate) use matterfast_markdown as markdown;
mod notifications;
#[cfg(target_os = "android")]
mod open_file;
pub(crate) use matterfast_paths as paths;
#[cfg_attr(not(target_os = "linux"), path = "playback_stub.rs")]
mod playback;
mod resource_cache;
mod runtime;
mod session;
mod state;
mod store;
mod themes;
pub(crate) use matterfast_timefmt as timefmt;
mod ui;
#[cfg(any(windows, test))]
mod url_scheme;
mod video;

pub const APP_ID: &str = "app.akergez.Matterfast";

/// The desktop entry point: what `main` is.
#[cfg(not(target_os = "android"))]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "matterfast=debug,mattermost_api=debug,mattermost_calls=debug".into()
            }),
        )
        .init();

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
        return;
    }
    let later_launches = unique.then(ipc::listen).flatten();
    // Only the copy people actually use claims the sign-in links: a demo or
    // a profiling run must not take them away from it.
    #[cfg(windows)]
    if unique {
        url_scheme::register();
    }

    let application = gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        // Closing the window keeps the process, the socket and the
        // notifications alive — a chat client you have to keep a window open
        // for is not one. Quitting is therefore always explicit.
        .with_quit_mode(gpui_kit::QuitMode::Explicit);

    // macOS does not start a second copy to deliver a link: it sends the link
    // to the running one as an event, and the same goes for a click on the
    // Dock icon. Both are what a later launch is everywhere else.
    #[cfg(target_os = "macos")]
    let later_links = {
        let (links, later_links) = async_channel::unbounded::<String>();
        application.on_open_urls(move |urls| {
            for url in urls {
                let _ = links.try_send(url);
            }
        });
        application.on_reopen(|cx| ui::handle_request(ipc::Request::Activate, cx));
        later_links
    };

    application.run(move |cx| {
        start(requests, cx);
        if let Some(later_launches) = later_launches {
            runtime::receive(later_launches, |request, cx| {
                ui::handle_request(request, cx);
                true
            });
        }
        #[cfg(target_os = "macos")]
        runtime::receive(later_links, |link, cx| {
            for request in ipc::requests_from_args(std::iter::once(link)) {
                ui::handle_request(request, cx);
            }
            true
        });
    });
}

/// What every platform does once the toolkit is running.
fn start(requests: Vec<ipc::Request>, cx: &mut gpui_kit::App) {
    gpui_kit::init(cx);
    // On Android `launch::run` has done this already.
    #[cfg(not(target_os = "android"))]
    gpui_android_glue::init(cx);
    fonts::install(cx);
    // Noted before a theme is shown: these are the fonts to go back to.
    appearance::built_in_fonts(cx);
    themes::init(cx);
    appearance::apply(None, cx);
    runtime::install(cx);
    ui::init(cx);

    for request in requests {
        ui::handle_request(request, cx);
    }
}

/// Hands a link to whatever the system opens links with. Everything goes
/// through here rather than to the toolkit, because the Android platform's
/// own `open_url` does nothing at all.
pub(crate) use gpui_android_glue::open_url;

/// The Android entry point, called by `android-activity` on a thread of its
/// own once the activity has loaded this library. `launch::run` does what
/// every application does — logcat, the private directory for the XDG
/// variables (and the cache: attachments and pictures are kept there), the
/// certificate verifier — and returns never.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: gpui_android_glue::launch::AndroidApp) {
    use gpui_android_glue::launch::{run, Config};

    let config = Config::new("matterfast")
        .filter("info,matterfast=debug,mattermost_api=debug,mattermost_calls=debug")
        .with_cache();
    run(app, config, gpui_kit::assets::AllAssets, |cx| {
        // Before anything can reach rustls. Two providers are in the tree —
        // aws-lc-rs through reqwest, ring through webrtc's DTLS — so rustls
        // refuses to pick one, and the thread that first needs TLS panics.
        mattermost_api::tls::install_crypto_provider();

        // The browser finishes a single sign-on by opening the activity with
        // a `mattermost-dev://` link, which is this launch's argument. A
        // notification left over from a process that is gone opens it too,
        // and that is only a launch: there is no session yet for it to point
        // into.
        let link = gpui_mobile::packages::deeplink::get_initial_link()
            .ok()
            .flatten()
            .filter(|link| !link.starts_with("matterfast-notice:"));
        let requests = ipc::requests_from_args(link.into_iter());

        // And when the activity is already running, the link comes to it
        // instead, on the system's thread: this is what a second launch is on
        // a desktop.
        let (links, later_links) = async_channel::unbounded::<String>();
        gpui_mobile::packages::deeplink::set_deep_link_handler(move |link| {
            // A pressed notification arrives the same way.
            if notifications::pressed(&link.to_string()) {
                return;
            }
            let _ = links.try_send(link.to_string());
        });

        start(requests, cx);
        runtime::receive(later_links, |link, cx| {
            for request in ipc::requests_from_args(std::iter::once(link)) {
                ui::handle_request(request, cx);
            }
            true
        });
    });
}
