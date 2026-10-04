//! The rules of a message row — who reacted, how a run of joins reads, how big
//! a picture is drawn. None of it knows there is a window.

mod custom_status;
mod image_size;
mod link_target;
mod markdown_text;
mod mention_sigils;
mod plural;
mod reaction_names;
mod reaction_tooltip;
mod replace_word;
mod system_messages;
#[cfg(test)]
mod test_support;

pub use custom_status::{custom_status_tooltip, status_is_live};
pub use image_size::{image_size, scaled_size, ImageSize};
pub use link_target::{link_target, LinkTarget};
pub use markdown_text::message_markdown;
pub use plural::plural;
pub use system_messages::system_lines;

pub(crate) use reaction_names::reaction_names;
pub(crate) use reaction_tooltip::reaction_tooltip;
