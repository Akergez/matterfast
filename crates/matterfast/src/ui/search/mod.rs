//! The message search box in the title bar, and what it suggests.
//!
//! Mattermost searches by a line of text with modifiers in it — `from:anna
//! in:town-square before:2026-10-03 release` — and the server is what reads
//! that line; nothing here parses a search. What this module does is make the
//! grammar findable: an empty box lists the modifiers, a modifier lists what
//! can follow it (people, channels, a few dates), and picking a row writes it
//! into the line the way it would have been typed.
//!
//! Like the composer's completion list ([`super::autocomplete`]) it never
//! talks to the network: the box reports the word under the cursor, the
//! session answers with rows. Working out what is being typed and writing a
//! pick back are plain functions, tested without a window.
//!
//! Unlike that list, nothing is selected until an arrow key says so: Enter in
//! a search box means "search", and must not turn into "from:" because a list
//! happened to be open.

mod accept;
mod build;
mod channels;
mod constants;
mod dates;
mod hint;
mod hint_at;
mod hint_list;
mod modifiers;
mod people;
mod render;
mod search_box;
mod suggestion;
mod suggestions;
mod typing;
mod word_start;

pub use people::person;
pub use render::render;
pub use search_box::SearchBox;
pub(crate) use build::build;
