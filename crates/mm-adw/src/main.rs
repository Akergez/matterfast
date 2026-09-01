//! A native Mattermost client for GNOME.
//!
//! The window is a genuine three-pane layout — team rail, channel sidebar,
//! conversation — built from two nested [`adw::NavigationSplitView`]s so that
//! it collapses the way GNOME expects: the conversation takes over first, then
//! the rail folds away on a phone-sized window.

mod audio;
mod avatars;
mod demo;
mod emoji;
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

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::empty())
        .build();

    app.connect_startup(|_| {
        load_css();
        load_icons();
    });
    app.connect_activate(ui::build_window);
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
