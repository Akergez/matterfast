use std::rc::Rc;

use gpui_adaptive::{FieldEvent, TextFieldState};
use gpui_kit::{App, AppContext, Entity, Window};

use crate::ui::Ui;

/// Builds the box and wires what it reports to the session. On a pointer
/// Enter sends and Shift+Enter breaks the line; on a finger Enter breaks the
/// line and the send button sends.
pub(crate) fn build_composer(
    ui: &Rc<Ui>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<TextFieldState>, gpui_kit::Subscription) {
    let composer = cx.new(|cx| {
        let mut composer = TextFieldState::prompt(1, 6, window, cx);
        composer.set_placeholder("Reply…", window, cx);
        composer
    });
    let weak = Rc::downgrade(ui);
    let subscription = cx.subscribe(&composer, move |_, event: &FieldEvent, cx| {
        let Some(ui) = weak.upgrade() else { return };
        match event {
            FieldEvent::Change => ui.later(cx, |ui, cx| ui.right.composer_changed(ui, cx)),
            FieldEvent::Submit => ui.later(cx, |ui, cx| ui.right.composer_submitted(ui, cx)),
            _ => {}
        }
    });
    (composer, subscription)
}
