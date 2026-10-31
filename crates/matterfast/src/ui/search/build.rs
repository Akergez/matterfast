use std::rc::Rc;

use gpui_adaptive::{FieldEvent, Purpose, TextFieldState};
use gpui_kit::{App, AppContext, Entity, Subscription, Window};

use crate::ui::Ui;

/// Builds the search box and wires what it reports to the session.
pub(crate) fn build(
    ui: &Rc<Ui>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<TextFieldState>, Subscription) {
    let input = cx.new(|cx| {
        let mut input = TextFieldState::new(Purpose::Plain, window, cx);
        input.set_placeholder("Search messages", window, cx);
        input
    });
    let weak = Rc::downgrade(ui);
    let subscription = cx.subscribe(&input, move |_, event: &FieldEvent, cx| {
        let Some(ui) = weak.upgrade() else { return };
        match event {
            FieldEvent::Change => ui.later(cx, |ui, cx| ui.search_box.changed(ui, cx)),
            // On Enter, not on every keystroke: a post search is a round trip
            // to the server, and searching per character would be a request
            // per character.
            FieldEvent::Submit => ui.later(cx, |ui, cx| ui.search_box.submit(ui, cx)),
            // The field can live inside a cached sidebar pane. Redraw its
            // owner so `render` reads the actual window focus and updates hints.
            FieldEvent::Focus | FieldEvent::Blur => crate::ui::refresh(cx),
        }
    });
    (input, subscription)
}
