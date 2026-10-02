//! A native Mattermost client.
//!
//! The window is a three-pane layout — channel sidebar, conversation, and a
//! thread/inbox panel — drawn with GPUI. See [`ui`] for the session and the
//! panes, and [`ipc`] for how a second launch finds the first.

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
mod playback;
mod resource_cache;
mod runtime;
mod session;
mod state;
mod store;
mod timefmt;
mod ui;
mod video;

pub const APP_ID: &str = "io.gitlab.akergez.Matterfast";

fn main() {
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

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        // Closing the window keeps the process, the socket and the
        // notifications alive — a chat client you have to keep a window open
        // for is not one. Quitting is therefore always explicit.
        .with_quit_mode(gpui_kit::QuitMode::Explicit)
        .run(move |cx| {
            gpui_kit::init(cx);
            fonts::install(cx);
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
