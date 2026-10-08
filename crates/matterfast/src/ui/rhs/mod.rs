//! The right-hand panel: a thread, or search results.
//!
//! Mattermost puts these in the same place and swaps between them, which is
//! worth copying — they are alternatives, never side by side, and sharing
//! one surface keeps the window from growing a fifth column.
//!
//! The inbox is kept here too — what it lists is built beside the thread it
//! opens — but it is drawn on the other side of the window, as a tab of the
//! list of conversations ([`inbox_view`]).

mod build_composer;
mod build_inbox;
mod build_thread_rows;
mod draft;
mod inbox;
mod inbox_row;
mod inbox_row_view;
mod inbox_view;
mod panel_mode;
mod refresh;
mod render;
mod right_panel;
mod search_row;
mod search_view;
mod target;
mod thread_row;
mod thread_view;

pub use panel_mode::PanelMode;
pub use render::render;
pub use right_panel::RightPanel;
pub(crate) use build_composer::build_composer;
pub(crate) use inbox_view::inbox_view;
