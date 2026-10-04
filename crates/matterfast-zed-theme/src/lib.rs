//! Reading a theme written for the Zed editor.
//!
//! Zed's is the theme format people actually publish in — several hundred
//! families in its extension registry — while the toolkit has one of its own
//! that almost nobody writes for. The two agree on how code is highlighted
//! (the toolkit copied that part from Zed on purpose) and on nothing else, so
//! this crate is the dictionary between them: a Zed theme family goes in as
//! text, a document in the toolkit's format comes out.
//!
//! It is a translation between two vocabularies that do not line up, and it
//! takes sides where they differ:
//!
//! - Zed is an editor, and its `background` is the frame *around* the editor.
//!   What a chat window is mostly made of is the equivalent of the editor
//!   itself, so `editor.background` is the background here and the panels
//!   become the sidebar. The title bar is the sidebar's colour and not Zed's
//!   own for it: this window has two surfaces, the frame and the page.
//! - Zed has no "primary" colour, the one a default button is filled with.
//!   `text.accent` is the nearest thing a theme author chose on purpose, and
//!   the text on top of it is the background colour, which contrasts with an
//!   accent for the same reason the accent contrasts with the background.
//! - About thirty of the toolkit's hundred and fifty colours are named. The
//!   rest are derived by the toolkit from those, which is also what its own
//!   themes rely on.
//!
//! Everything works on JSON values and nothing here knows about the toolkit's
//! types, so a file somebody wrote by hand can be wrong in any way it likes:
//! a colour that is not a colour is dropped, never passed on to fail later.

mod color;
mod colors;
mod convert;
mod error;
mod highlight;
mod is_zed;
mod opaque;
mod parse;
mod relax;
mod surfaces;
mod syntax_style;
#[cfg(test)]
mod test_support;
mod theme;

pub use convert::convert;
pub use error::Error;
pub use is_zed::is_zed;
pub use parse::parse;
