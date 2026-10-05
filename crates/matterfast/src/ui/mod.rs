//! The user interface: the panes, and the session that drives them.
//!
//! The panes do not mutate state directly. They dispatch an [`Action`], which
//! is queued and applied once whatever was running has finished. That keeps
//! the borrow of `RefCell<AppState>` short and obviously non-overlapping, which
//! is otherwise the standard way an `Rc<RefCell<…>>` application panics at
//! runtime.
//!
//! [`Ui`] is the session. It outlives its window: closing the window while the
//! application keeps running in the background drops what was on screen and
//! nothing else, so the socket, the call and the notifications carry on, and a
//! new window attaches to the same session. For that reason nothing in here
//! holds a window — it asks for one, through [`WindowSlot`], when it needs to
//! do something only a window can.

mod account;
mod action;
mod autocomplete;
mod call_dock;
pub(crate) mod channel_icon;
mod chat;
mod constants;
mod dialogs;
mod group;
mod interactive;
mod kit;
mod layout;
mod lightbox;
pub mod login;
mod media;
mod mentions;
mod menu_action;
mod message;
mod notify;
mod profile;
mod rhs;
mod script;
mod search;
mod session;
mod settings;
mod shell;
mod sidebar;
pub mod sso;
mod storage;
mod switcher;

pub use shell::{current, handle_request, init};

pub(crate) use action::Action;
pub(crate) use layout::{Divider, WindowSlot};
pub(crate) use menu_action::MenuAction;
pub(crate) use session::{bootstrap, Ui};
pub(crate) use shell::{redraw, Part};
