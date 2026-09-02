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
mod cache;
mod demo;
mod emoji;
mod markdown;
mod runtime;
mod session;
mod state;
mod ui;
mod video;

use adw::prelude::*;
use gtk::gio;

pub const APP_ID: &str = "ru.toxblh.MattermostAdw";

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

    app.connect_startup(|_| {
        load_css();
        load_icons();
    });
    app.connect_activate(ui::build_window);
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
