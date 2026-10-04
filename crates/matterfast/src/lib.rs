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
mod zed_extensions;
pub(crate) use matterfast_zed_theme as zed_theme;

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
    fonts::install(cx);
    themes::load(cx);
    appearance::apply(None, cx);
    runtime::install(cx);
    ui::init(cx);

    for request in requests {
        ui::handle_request(request, cx);
    }
}

/// Hands a link to whatever the system opens links with.
///
/// Everything goes through here rather than to the toolkit, because the
/// Android platform's own `open_url` does nothing at all.
pub(crate) fn open_url(url: &str, cx: &mut gpui_kit::App) {
    #[cfg(not(target_os = "android"))]
    cx.open_url(url);
    #[cfg(target_os = "android")]
    {
        let _ = cx;
        match gpui_mobile::packages::url_launcher::launch_url(url) {
            Ok(true) => {}
            Ok(false) => tracing::error!("nothing on this phone opens {url}"),
            Err(error) => tracing::error!(%error, "could not open {url}"),
        }
    }
}

/// The Android entry point, called by `android-activity` on a thread of its
/// own once the activity has loaded this library. It returns when the
/// activity is destroyed.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    use gpui_mobile::android::jni;
    use std::sync::atomic::{AtomicBool, Ordering};

    // The system may create the activity again in a process it has kept, and
    // that calls this again, on another thread. The toolkit belongs to the
    // thread it first ran on and cannot be started twice, so one process is
    // one run: a second call ends the process, and the system starts the
    // activity over in a fresh one.
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        std::process::exit(0);
    }

    // There is no terminal: `tracing` hands its events to `log` when nothing
    // subscribes, and this sends `log` to logcat.
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("matterfast")
            .with_filter(
                android_logger::FilterBuilder::new()
                    .parse("info,matterfast=debug,mattermost_api=debug,mattermost_calls=debug")
                    .build(),
            ),
    );
    jni::install_panic_hook();

    // An application has no home directory here, only the private directory
    // the system gives it; `paths` reads these on first use, and nothing has
    // used it yet.
    if let Some(files) = app.internal_data_path() {
        // SAFETY: no other thread of ours exists yet to read the environment.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", files.join("config"));
            std::env::set_var("XDG_DATA_HOME", files.join("data"));
            std::env::set_var("XDG_CACHE_HOME", files.join("cache"));
        }
    }

    mattermost_api::tls::install_crypto_provider();

    jni::init_platform(&app);
    let Some(platform) = jni::shared_platform() else {
        tracing::error!("no Android platform to run on");
        return;
    };

    // Certificates are checked by the system, which is Java: the verifier has
    // to be given the activity before the first connection, or that
    // connection panics.
    if let Err(error) = jni::with_env(|env| {
        let activity = jni::activity(env)?;
        rustls_platform_verifier::android::init_with_env(env, activity)
            .map_err(|error| error.to_string())
    }) {
        tracing::error!(%error, "no certificate verifier: nothing will connect");
    }

    // The browser finishes a single sign-on by opening the activity with a
    // `mattermost-dev://` link, which is this launch's argument.
    // A notification left over from a process that is gone opens it too, and
    // that is only a launch: there is no session yet for it to point into.
    let link = gpui_mobile::packages::deeplink::get_initial_link()
        .ok()
        .flatten()
        .filter(|link| !link.starts_with("matterfast-notice:"));
    let requests = ipc::requests_from_args(link.into_iter());

    // And when the activity is already running, the link comes to it instead,
    // on the system's thread: this is what a second launch is on a desktop.
    let (links, later_links) = async_channel::unbounded::<String>();
    gpui_mobile::packages::deeplink::set_deep_link_handler(move |link| {
        // A pressed notification arrives the same way.
        if notifications::pressed(&link.to_string()) {
            return;
        }
        let _ = links.try_send(link.to_string());
    });

    gpui_kit::gpui::Application::with_platform(platform.into_rc())
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            start(requests, cx);
            runtime::receive(later_links, |link, cx| {
                for request in ipc::requests_from_args(std::iter::once(link)) {
                    ui::handle_request(request, cx);
                }
                true
            });
        });

    // The activity is gone. Nothing of ours can be used by the next one (see
    // above), so the process goes with it.
    std::process::exit(0);
}
