//! Mattermost messages are Markdown — almost.
//!
//! The text view renders standard Markdown, and a message is that plus three
//! things Mattermost writes as text but means as something else:
//!
//! * `:shortcode:` is an emoji,
//! * `@handle` is a person, and worth showing by name,
//! * a single newline is a line break, where Markdown reads it as a space.
//!
//! So this is not a renderer. It rewrites the message into Markdown that says
//! what was meant, and leaves the drawing to the view. Everything it does not
//! recognise passes through byte for byte — a message is arbitrary text from
//! another person, and the safest thing to do with the parts that are not ours
//! is nothing.
//!
//! The rewriting only touches prose. Code, inline or fenced, is somebody
//! *showing* a shortcode or a handle rather than using it.

mod constants;
mod escape_label;
mod inline;
mod inside_url;
mod prepare_full;
mod prepare_with;
mod preview;
mod resolve_handle;
mod sigil;
#[cfg(test)]
mod test_support;

pub use constants::{EMOJI_SCHEME, MENTION_SCHEME};
pub use prepare_full::prepare_full;
pub use prepare_with::prepare_with;
pub use preview::preview;
pub use sigil::Sigil;
