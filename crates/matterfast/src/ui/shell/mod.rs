//! The window, and what is in it at each stage of a launch.
//!
//! The window is a three-pane layout — channel sidebar, conversation, and a
//! thread/inbox panel. It collapses in two steps as it narrows: the thread
//! panel is laid over the conversation first, then the channel list becomes a
//! page of its own that the conversation sits in front of. Under the sidebar
//! is the call dock, which is pinned there for as long as a call runs and
//! moves under the conversation once the sidebar is a separate page.
//!
//! The layout is read off the window's width on every frame, which is the
//! only thing that is true when a window is tiled, maximized or dragged onto
//! a phone-sized screen. The one thing kept is how wide the two side columns
//! were dragged (`Widths`), and that is a wish the window's width overrules.

mod actions;
mod app_icon;
mod choose_server;
mod column_width;
mod constants;
mod divider;
mod init;
mod matterfast;
mod open_window;
mod panes;
mod present;
mod pretty_server;
mod render;
mod session_panes;
mod sessions;
mod shell_view;
mod signed_out;
mod stage;
mod startup;
mod system_bars;
mod title_bar;
mod videos;
mod window_size;
mod with_shell;

pub use init::init;
pub use matterfast::current;
pub use panes::{redraw, refresh, Part};
pub use present::{handle_request, present};
pub use signed_out::signed_out;
