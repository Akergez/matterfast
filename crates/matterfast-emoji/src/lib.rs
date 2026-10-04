//! Turning Mattermost emoji shortcodes into something a person can read.
//!
//! Mattermost stores reactions as bare names — `tada`, `+1`, `eyes` — and it is
//! the client's job to render them. Most names come from `emoji-data` and match
//! the gemoji shortcodes the [`emojis`] crate indexes, but not all: Mattermost
//! kept the longer Unicode CLDR names for a handful of faces where gemoji uses
//! a shorter alias. Those are listed in `aliases`.
//!
//! Anything still unresolved is a **custom** emoji the server hosts as an
//! image; there is no Unicode for it, so the caller falls back to the image or
//! to the literal `:name:`.

mod aliases;
mod found;
mod label;
mod quick_reactions;
mod rendered;
mod resolve;
mod search;
mod skin_tones;

pub use found::Found;
pub use label::label;
pub use quick_reactions::QUICK_REACTIONS;
pub use rendered::Rendered;
pub use resolve::resolve;
pub use search::search;
