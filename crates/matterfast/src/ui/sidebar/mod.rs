//! The list of conversations: an account/team switcher in the header, the
//! folders as tabs under it, and the list below.
//!
//! It is one list, as in Telegram: channels and direct messages together,
//! each row saying what was said there last. The server's sidebar categories
//! are the folders the list can be narrowed to, not headings inside it, and
//! the order is the categories' own: what is new is the inbox, the first
//! tab, and the lists beside it stay where the person put them.

mod call_badge;
mod channel_row;
mod channel_sidebar;
mod folders;
mod main_menu;
mod preview;
mod render;
mod row_action;
mod row_menu;
mod switcher;
mod tab_swipe;

pub(crate) use channel_row::{
    channel_row, Fit, INBOX_ANSWERS, INBOX_FACE, INBOX_HEADING, INBOX_ROW, INBOX_SAID,
};
pub(crate) use folders::turn;
pub use channel_sidebar::ChannelSidebar;
pub use render::render;
pub use row_action::RowAction;
