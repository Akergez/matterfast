//! The call dock: a strip pinned to the bottom of the sidebar for as long as
//! you are in a call.
//!
//! A call outlives the channel you started it in — you can read somewhere
//! else while it runs — so its controls cannot live in the conversation
//! header. They live here, where they are reachable from every channel, and
//! the dock doubles as the way back to the call's own channel.
//!
//! It sits under the channel list, outside anything that scrolls or switches,
//! which is why it survives navigation; on a window too narrow to show the
//! sidebar beside the conversation it moves under the conversation instead.

mod caption_line;
mod constants;
mod dock;
mod host_action;
mod host_controls;
mod host_icon;
mod render;
mod roster;
mod summary;
mod toggle;

pub use dock::CallDock;
pub use host_action::HostAction;
pub use render::render;
