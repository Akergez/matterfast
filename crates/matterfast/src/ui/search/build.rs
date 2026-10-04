use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{App, AppContext, Entity, Subscription, Window};

use crate::ui::Ui;

/// Builds the search box and wires what it reports to the session.
pub(crate) fn build(
    ui: &Rc<Ui>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<InputState>, Subscription) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search messages"));
    let weak = Rc::downgrade(ui);
    let subscription = cx.subscribe(&input, move |_, event: &InputEvent, cx| {
        let Some(ui) = weak.upgrade() else { return };
        match event {
            InputEvent::Change => ui.later(cx, |ui, cx| ui.search_box.changed(ui, cx)),
            // On Enter, not on every keystroke: a post search is a round trip
            // to the server, and searching per character would be a request
            // per character.
            InputEvent::PressEnter { .. } => ui.later(cx, |ui, cx| ui.search_box.submit(ui, cx)),
            // Where the cursor is, is read off the window when the box is
            // drawn: see `SearchBox::focused`.
            InputEvent::Focus | InputEvent::Blur => {}
        }
    });
    (input, subscription)
}
