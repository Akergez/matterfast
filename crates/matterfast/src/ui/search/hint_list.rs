use gpui_adaptive::prim;
use gpui_adaptive::ActiveColors as _;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, px, relative, AnyElement, App, FontWeight, MouseButton};

use super::suggestions::heading;
use crate::ui::Ui;

/// The list under the box.
pub(super) fn hint_list(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let search = &ui.search_box;
    let colors = cx.colors();
    let title = search.hint.borrow().as_ref().map_or("", heading);
    let mut rows = prim::column()
        .id("search-hints")
        .absolute()
        // Under the box: its place is counted from the box's own top.
        .top(relative(1.))
        .mt_1()
        .left_0()
        .w_full()
        .min_w(px(320.))
        .p_1()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.raised)
        .text_color(colors.text)
        .shadow_md()
        .occlude()
        .child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(colors.muted)
                .child(title),
        );
    let selected = search.selected.get();
    for (index, row) in search.rows.borrow().iter().enumerate() {
        rows = rows.child(
            prim::row()
                .id(("hint", index))
                .gap_2()
                .px_2()
                .h(px(28.))
                .items_center()
                .rounded_sm()
                .text_sm()
                .cursor_pointer()
                .when(selected == Some(index), |row| row.bg(colors.hover))
                .hover(|style| style.bg(colors.hover))
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::MEDIUM)
                        .child(row.label.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_right()
                        .text_xs()
                        .text_color(colors.muted)
                        .child(row.detail.clone()),
                )
                // On the press, not the click: the press is also what takes
                // the focus out of the box, and with it the list.
                .on_mouse_down(MouseButton::Left, {
                    let ui = ui.clone();
                    move |_, window, cx| {
                        cx.stop_propagation();
                        window.prevent_default();
                        ui.later(cx, move |ui, cx| ui.search_box.pick(index, ui, cx));
                    }
                }),
        );
    }
    rows.into_any_element()
}
