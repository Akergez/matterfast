//! One message row, shared by the channel feed and the thread panel.
//!
//! Both views draw the same thing with small differences (a thread never shows
//! a "N replies" footer, because you are already in the thread), so the
//! rendering lives here and the differences are parameters.
//!
//! `rules` is the part with rules in it — who reacted, how a run of joins
//! reads, how big a picture is drawn — and none of it knows there is a
//! window. `view` draws.

mod post_action;
mod row_options;
mod rules;
mod view;

pub use post_action::PostAction;
pub use row_options::{RowOptions, GROUPING_WINDOW_MS};
pub use rules::{
    custom_status_tooltip, link_target, message_markdown, plural,
    status_is_live, system_lines, LinkTarget,
};
pub use view::{emoji_element, markdown, row, system_block};
pub(super) use view::audience;
