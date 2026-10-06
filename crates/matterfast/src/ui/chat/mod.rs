//! Pane 3: the conversation — header, call banner, message list, composer.
//!
//! [`ChatView`] is what the rest of the application talks to: plain state the
//! pane is drawn from, plus the two things that only exist while there is a
//! window — the composer and the scrolling list. [`render`] turns it into
//! elements, on every frame, from whatever that state says right now.

mod feed;
mod render;
mod scroll_trace;
mod status_lines;
mod view;

pub use render::render;
pub use view::ChatView;
pub(crate) use render::{build_composer, completion_keys, completion_list};
pub(crate) use scroll_trace::scroll_trace_enabled;
