use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{App, AppContext, Entity, Window};

use crate::ui::Ui;

/// Builds the composer and wires what it reports to the session.
pub(crate) fn build_composer(
    ui: &Rc<Ui>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<TextareaState>, gpui_kit::Subscription) {
    let composer = cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(1, 8)
            // Enter sends; Shift+Enter inserts a newline.
            .submit_on_enter(true)
            .placeholder("Write a message…")
    });
    let weak = Rc::downgrade(ui);
    let subscription = cx.subscribe(&composer, move |_, event: &InputEvent, cx| {
        let Some(ui) = weak.upgrade() else { return };
        match event {
            InputEvent::Change => ui.later(cx, |ui, cx| ui.chat.composer_changed(ui, cx)),
            InputEvent::PressEnter { shift: false, .. } => {
                ui.later(cx, |ui, cx| ui.chat.composer_submitted(ui, cx))
            }
            _ => {}
        }
    });
    (composer, subscription)
}
