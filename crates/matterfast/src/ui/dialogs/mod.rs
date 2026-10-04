//! The dialogs that ask a question and hand the answer back.
//!
//! Nothing here talks to the server or to [`crate::state`]: each function takes
//! a callback and calls it with what the person chose. That keeps the API calls
//! in the session, where the client and the action queue already live, and it
//! means these can be read (and moved) without tracing a request through them.
//!
//! There are really only two shapes. A **form** is some fields and a button:
//! make a channel, set a status, pick a time. A **list** is rows that each
//! offer something, optionally under a search box: browse channels, see who is
//! here, look through what you scheduled. Everything else here is one of those
//! two with its fields or its rows filled in.
//!
//! Every callback is run *after* the click that caused it has finished being
//! handled, never during — the session code they call may want the window, and
//! a click is the window in the middle of something.

mod account_notifications;
mod bookmark_list;
mod callback;
mod channel_browser;
mod channel_notifications;
mod choose;
mod confirm;
mod confirm_archive;
mod confirm_leave;
mod confirm_remove_member;
mod create_channel;
mod edit_channel;
mod emoji_picker;
mod footer;
mod forms;
mod levels;
mod list;
mod member_list;
mod moments;
mod name_category;
mod picker;
mod post_reminder;
mod schedule_message;
mod slugify;
mod team_browser;

pub use account_notifications::account_notifications;
pub use bookmark_list::BookmarkList;
pub use channel_browser::ChannelBrowser;
pub use channel_notifications::channel_notifications;
pub use confirm::confirm;
pub use confirm_archive::confirm_archive;
pub use confirm_leave::confirm_leave;
pub use confirm_remove_member::confirm_remove_member;
pub use create_channel::create_channel;
pub use edit_channel::edit_channel;
pub use emoji_picker::pick_emoji;
pub(crate) use footer::footer;
pub use forms::{Field, Form};
pub use list::{show_rows, Row};
pub use member_list::MemberList;
pub use name_category::name_category;
pub use picker::Picker;
pub use post_reminder::post_reminder;
pub use schedule_message::schedule_message;
pub use team_browser::TeamBrowser;
