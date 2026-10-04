use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, AnyElement, ElementId, ObjectFit};

use crate::emoji;
use crate::ui::Ui;

/// One emoji, however it has to be drawn: a Unicode glyph, or a custom upload
/// as a small picture. A custom one that has not arrived shows its shortcode,
/// which is at least readable.
pub fn emoji_element(ui: &Rc<Ui>, name: &str, size: f32) -> AnyElement {
    match emoji::resolve(name) {
        emoji::Rendered::Unicode(glyph) => div().child(glyph).into_any_element(),
        emoji::Rendered::Custom => match ui.avatars.custom_emoji(name) {
            // The id is what lets an animated one play; see `CustomEmoji`.
            Some(picture) => img(picture)
                .id(ElementId::Name(format!("emoji-{name}").into()))
                .size(px(size))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div().child(format!(":{name}:")).into_any_element(),
        },
    }
}
