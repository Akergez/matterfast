use std::rc::Rc;

use gpui_kit::component::input::{Enter, Escape, IndentInline, Input, MoveDown, MoveUp};
use gpui_kit::component::{v_flex, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{deferred, div, px, AnyElement, App, Focusable, MouseButton, Window};

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
    let focused = input.read(cx).focus_handle(cx).is_focused(window);
    if search.focused.replace(focused) != focused && focused {
        ui.later(cx, |ui, cx| ui.search_box.changed(ui, cx));
    }
    v_flex()
        .id("title-search")
        .relative()
        .flex_none()
        .w(px(width))
        // The bar under it is what drags the window.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        // The list has first refusal on these keys while a row of it is
        // chosen, and on the arrows whenever it is open.
        .capture_action({
            let ui = ui.clone();
            move |_: &MoveUp, _, cx| {
                if ui.search_box.is_open() {
                    ui.search_box.step(-1);
                    cx.stop_propagation();
                    crate::ui::refresh(cx);
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &MoveDown, _, cx| {
                if ui.search_box.is_open() {
                    ui.search_box.step(1);
                    cx.stop_propagation();
                    crate::ui::refresh(cx);
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &Enter, _, cx| {
                let chosen = ui.search_box.selected.get();
                if let Some(index) = chosen.filter(|_| ui.search_box.is_open()) {
                    cx.stop_propagation();
                    ui.later(cx, move |ui, cx| ui.search_box.pick(index, ui, cx));
                }
            }
        })
        // Tab takes the chosen row, or the first: it is the key for "yes,
        // that one" and has nothing else to do in a one-line box.
        .capture_action({
            let ui = ui.clone();
            move |_: &IndentInline, _, cx| {
                if ui.search_box.is_open() {
                    let index = ui.search_box.selected.get().unwrap_or(0);
                    cx.stop_propagation();
                    ui.later(cx, move |ui, cx| ui.search_box.pick(index, ui, cx));
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &Escape, _, cx| {
                if ui.search_box.is_open() {
                    ui.search_box.close();
                    cx.stop_propagation();
                    crate::ui::refresh(cx);
                }
            }
        })
        .child(
            Input::new(&input)
                .small()
                .prefix(Lucide::Search)
                .cleanable(true),
        )
        .when(search.is_open(), |field| {
            // Deferred, so that it is drawn over the panes under the bar
            // rather than under them.
            field.child(deferred(hint_list(ui, cx)).with_priority(1))
        })
        .into_any_element()
}
