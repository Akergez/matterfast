//! The fonts the application brings with it.
//!
//! Two things the text renderer cannot do with what a current desktop ships,
//! both found the hard way (see `assets/fonts/README.md`):
//!
//! * a **variable** UI font is loaded as one regular face, so nothing drawn
//!   bold actually is — and the default UI fonts on Fedora are variable;
//! * a **COLRv1** emoji font is not drawn at all, and that is what Fedora's
//!   Noto Color Emoji is. Every emoji came out as a blank space.
//!
//! So the application carries static faces of Inter for its own text, and a
//! COLRv0 emoji font it can actually draw.

use std::borrow::Cow;

use gpui_kit::component::Theme;
use gpui_kit::{font, App};

/// The family the bundled UI faces belong to.
const UI_FAMILY: &str = "Inter";

/// The family the renderer's fallback list looks for when a character turns
/// out to be an emoji. The bundled emoji font answers to this name.
const EMOJI_FAMILY: &str = "Noto Color Emoji";

/// Registers the bundled fonts and points the theme at them. Call once, after
/// the component library is initialised and before any window is opened.
pub fn install(cx: &mut App) {
    // Asking for the system's emoji family *as a text font* makes the loader
    // discard its faces: it drops any family with no glyph for the letter
    // "m", which an emoji font never has. That is the point. The fallback
    // list finds an emoji font by family name alone, and with the system's
    // COLRv1 faces still registered under that name it would keep choosing
    // the one that draws nothing. After this, the bundled font added below is
    // the only face answering to the name.
    //
    // The answer itself is not wanted — the lookup falls through to the
    // default UI font — only the side effect is.
    let _ = cx.text_system().resolve_font(&font(EMOJI_FAMILY));

    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/NotoColorEmoji-Twemoji.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-BoldItalic.ttf")),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        // Text still draws, in whatever the desktop has; it is bold and emoji
        // that will be missing, which is worth saying once.
        tracing::warn!(%error, "could not load the bundled fonts");
        return;
    }
    Theme::global_mut(cx).font_family = UI_FAMILY.into();
}
