use gpui_adaptive::prim::column;
use std::rc::Rc;

use gpui_adaptive::{FieldKey, TextField};
use gpui_kit::prelude::*;
use gpui_kit::{deferred, div, px, AnyElement, App, MouseButton, Window};

use super::hint_list::hint_list;
use crate::ui::kit::Lucide;
use crate::ui::Ui;

/// The box as the title bar shows it, `width` wide, with its list under it.
pub fn render(ui: &Rc<Ui>, width: f32, window: &Window, cx: &mut App) -> AnyElement {
    let search = &ui.search_box;
    let Some(input) = search.input.borrow().clone() else {
        return div().into_any_element();
    };
    // The cursor arriving in the box is what opens the list, before anything
    // is typed; leaving it closes the list by `is_open` alone.
    let focused = input.read(cx).is_focused(window, cx);
    if search.focused.replace(focused) != focused && focused {
        ui.later(cx, |ui, cx| ui.search_box.changed(ui, cx));
    }
    column()
        .id("title-search")
        .relative()
        .flex_none()
        .w(px(width))
        // The bar under it is what drags the window.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        // The list has first refusal on these keys while a row of it is
        // chosen, and on the arrows whenever it is open.
        .child(TextField::new(&input).icon(Lucide::Search).cleanable().on_key({
            let ui = ui.clone();
            move |key, _, cx| {
                let search = &ui.search_box;
                match key {
                    FieldKey::Up | FieldKey::Down if search.is_open() => {
                        search.step(if key == FieldKey::Up { -1 } else { 1 });
                        crate::ui::refresh(cx);
                    }
                    FieldKey::Enter => {
                        let Some(index) = search.selected.get().filter(|_| search.is_open()) else {
                            return false;
                        };
                        ui.later(cx, move |ui, cx| ui.search_box.pick(index, ui, cx));
                    }
                    // Tab takes the chosen row, or the first: it is the key
                    // for "yes, that one" and has nothing else to do in a
                    // one-line box.
                    FieldKey::Tab if search.is_open() => {
                        let index = search.selected.get().unwrap_or(0);
                        ui.later(cx, move |ui, cx| ui.search_box.pick(index, ui, cx));
                    }
                    FieldKey::Escape if ui.split.collapsed.get() => {
                        search.collapse_compact(cx);
                        ui.channels.refresh(cx);
                    }
                    FieldKey::Escape if search.is_open() => {
                        search.close();
                        crate::ui::refresh(cx);
                    }
                    _ => return false,
                }
                true
            }
        }))
        .when(search.is_open(), |field| {
            // Deferred, so that it is drawn over the panes under the bar
            // rather than under them.
            field.child(deferred(hint_list(ui, cx)).with_priority(1))
        })
        .into_any_element()
}
