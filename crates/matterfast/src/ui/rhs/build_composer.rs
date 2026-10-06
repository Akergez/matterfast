use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{App, AppContext, Entity, Window};

use crate::ui::Ui;

/// Builds the reply box and wires what it reports to the session.
pub(crate) fn build_composer(
    ui: &Rc<Ui>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<TextareaState>, gpui_kit::Subscription) {
    let composer = cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(1, 6)
            .submit_on_enter(true)
            .placeholder("Reply…")
    });
    let weak = Rc::downgrade(ui);
    let subscription = cx.subscribe(&composer, move |composer, event: &InputEvent, cx| {
        crate::keyboard::follow(composer.entity_id(), event, crate::keyboard::Purpose::Message);
        let Some(ui) = weak.upgrade() else { return };
        match event {
            InputEvent::Change => ui.later(cx, |ui, cx| ui.right.composer_changed(ui, cx)),
            InputEvent::PressEnter { shift: false, .. } => {
                ui.later(cx, |ui, cx| ui.right.composer_submitted(ui, cx))
            }
            _ => {}
        }
    });
    (composer, subscription)
}
