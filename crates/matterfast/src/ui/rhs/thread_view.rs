use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, list, AnyElement, App};

use super::thread_row::ThreadRow;
use crate::ui::autocomplete::Composer;
use crate::ui::chat::{completion_keys, completion_list};
use crate::ui::kit::{self, Lucide};
use crate::ui::message::{self, RowOptions};
use crate::ui::Ui;

pub(super) fn thread_view(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let panel = &ui.right;
    if !panel.thread_loaded.get() {
        return div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("Loading thread…")
            .into_any_element();
    }

    let list_state = panel.list.clone();
    let row_ui = ui.clone();
    let rows = list(list_state.clone(), move |index, _, cx| {
        let rows = row_ui.right.thread.borrow();
        match rows.get(index) {
            Some(ThreadRow::Divider(label)) => div()
                .px_3()
                .pt_2()
                .child(kit::labelled_rule(label.clone(), cx.theme().border, cx))
                .into_any_element(),
            Some(ThreadRow::Post {
                post,
                grouped,
                body,
            }) => message::row(
                &row_ui,
                post,
                body,
                RowOptions {
                    grouped: *grouped,
                    // We are already in the thread.
                    show_thread_footer: false,
                    highlight: false,
                },
                cx,
            ),
            None => div().into_any_element(),
        }
    })
    .size_full()
    .py_3();

    let mut pane = v_flex().flex_1().min_h_0().child(
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .size_full()
                    .child(rows)
                    .vertical_scrollbar(&list_state),
            )
            // Over the bottom of the replies, just above the box it answers.
            .when_some(completion_list(ui, Composer::Thread, cx), |replies, list| {
                replies.child(list)
            }),
    );

    if let Some(composer) = panel.composer.borrow().clone() {
        pane = pane.child(
            h_flex()
                .flex_none()
                .gap_2()
                .px_3()
                .pt_1()
                .pb_3()
                .items_center()
                .child(
                    completion_keys(ui, Composer::Thread, div().flex_1().min_w_0()).child(
                        Textarea::new(&composer).on_paste({
                            let ui = ui.clone();
                            move |item, window, cx| {
                                crate::ui::paste_link::pasted(
                                    &ui,
                                    Composer::Thread,
                                    item,
                                    window,
                                    cx,
                                )
                            }
                        }),
                    ),
                )
                .child(
                    Button::new("reply")
                        .icon(Lucide::SendHorizontal)
                        .primary()
                        .tooltip("Reply  (Enter)")
                        .on_click(ui.click(|ui, cx| ui.right.submit(ui, cx))),
                ),
        );
    }
    pane.into_any_element()
}
