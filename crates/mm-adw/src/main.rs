//! A native Mattermost client for GNOME.
//!
//! The window is a three-pane layout — channel sidebar, conversation, and a
//! thread/inbox panel — built from an [`adw::NavigationSplitView`] wrapping an
//! [`adw::OverlaySplitView`], so it collapses the way GNOME expects: the
//! thread panel overlays first, then the sidebar folds away on a phone.
//! Under the sidebar sits the call dock, which is pinned there for as long as
//! a call runs and moves under the conversation once the sidebar folds away.

mod agents;
mod audio;
mod avatars;
mod background;
mod cache;
mod demo;
mod emoji;
mod markdown;
mod runtime;
mod session;
mod state;
mod ui;
mod video;

use std::cell::RefCell;

use adw::prelude::*;
use gtk::gio;

use background::Background;

pub const APP_ID: &str = "ru.toxblh.MattermostAdw";

thread_local! {
    /// The live hold, if the app is currently running windowless. It has to
    /// outlive every window, so it cannot live in one; GTK is single-threaded,
    /// so a thread-local is the whole of the storage needed.
    static HOLD: RefCell<Option<Background>> = const { RefCell::new(None) };
}

fn main() -> gtk::glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "mm_adw=debug,mattermost_api=debug,mattermost_calls=debug".into()
            }),
        )
        .init();

    // HANDLES_OPEN is what makes the `mattermost-dev://` SSO callback work: the
    // browser launches a second copy of this binary with the URI, GIO hands it
    // to the already-running one over D-Bus, and it arrives in `connect_open`.
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_startup(|app| {
        load_css();
        load_icons();
        add_actions(app);
    });
    app.connect_activate(present_or_build);

    // `window-removed` rather than `shutdown`: by the time shutdown runs the
    // application has already decided to exit, which is far too late to keep
    // it alive. This fires the moment a window leaves, which is exactly when
    // the hold has to be in place. GApplication only acts on a use count of
    // zero from the main loop, not inside the emission, so taking the hold
    // here still beats the exit.
    app.connect_window_removed(|app, window| {
        if app.windows().iter().any(|other| other != window) {
            return;
        }
        if !Background::enabled() {
            return;
        }
        tracing::info!("last window closed; staying in the background");
        HOLD.with_borrow_mut(|hold| *hold = Some(Background::hold(app)));
    });
    // A window holds the application by itself, so the hold is only wanted
    // between windows.
    app.connect_window_added(|_, _| {
        HOLD.with_borrow_mut(|hold| hold.take());
    });

    app.connect_open(|app, files, _hint| {
        // A cold start from the callback still needs a window to land in.
        if app.active_window().is_none() {
            ui::build_window(app);
        }
        if let Some(window) = app.active_window() {
            window.present();
        }
        for file in files {
            ui::sso::deliver(&file.uri());
        }
    });
    app.run()
}

/// Raising the application has to *show* something. Running in the background
/// means there may be no window at all, and a second launch of the binary —
/// clicking the launcher again — arrives here in the running instance. An
/// existing window is presented rather than duplicated: a second window would
/// mean a second session and a second websocket. `app.new-window` is the way
/// to ask for another one on purpose.
fn present_or_build(app: &adw::Application) {
    match app.active_window() {
        Some(window) => window.present(),
        None => ui::build_window(app),
    }
}

fn add_actions(app: &adw::Application) {
    let new_window = gio::SimpleAction::new("new-window", None);
    new_window.connect_activate({
        let app = app.clone();
        move |_, _| ui::build_window(&app)
    });
    app.add_action(&new_window);

    // Closing the window no longer quits, so something else has to. Dropping
    // the hold first is what lets the application actually go.
    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate({
        let app = app.clone();
        move |_, _| {
            HOLD.with_borrow_mut(|hold| hold.take());
            app.quit();
        }
    });
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
}

/// Makes the application icon findable.
///
/// An installed build picks it up from the XDG icon directories; a `cargo run`
/// from the source tree needs the tree's own `data/icons` on the search path.
fn load_icons() {
    gtk::Window::set_default_icon_name(APP_ID);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display)
            .add_search_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/icons"));
    }
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
