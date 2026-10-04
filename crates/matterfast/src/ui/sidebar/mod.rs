//! The channel sidebar: an account/team switcher in the header, the list below.

mod call_badge;
mod category_header;
mod channel_row;
mod channel_sidebar;
mod main_menu;
mod render;
mod row_action;
mod row_menu;
mod switcher;

pub use channel_sidebar::ChannelSidebar;
pub use render::render;
pub use row_action::RowAction;
