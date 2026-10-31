use gpui_adaptive::Command;
use gpui_adaptive::prim::spinner;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, list, px, AnyElement, App};

use super::completion_list::completion_list;
use super::render_item::render_item;
use crate::ui::kit::Lucide;
use crate::ui::Ui;

/// The feed itself: the list, its scrollbar, and what floats over it.
pub(super) fn feed(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let chat = &ui.chat;
    let list_state = chat.list.clone();
    let row_ui = ui.clone();
    let rows = list(list_state.clone(), move |index, _, cx| {
        let chat = &row_ui.chat;
        let items = chat.items.borrow();
        let highlight = chat.highlight.borrow();
        let began = std::time::Instant::now();
        let row = match items.get(index) {
            Some(item) => render_item(&row_ui, item, highlight.as_deref(), cx),
            // The list was told about a row the feed no longer has; one frame
            // later it will not be asked for.
            None => div().into_any_element(),
        };
        crate::ui::frame_log::row(crate::ui::Part::Chat, began);
        crate::ui::frame_log::timed("row", row)
    })
    .size_full()
    .py_3();

    // A block that stops short of the present is away from the newest
    // message wherever it is scrolled to, its own end included.
    let behind = chat.behind.get();
    let scrolled_up = behind
        || (!list_state.is_following_tail()
            && list_state.max_offset_for_scrollbar().y > px(0.)
            && !list_state.is_scrolled_to_end().unwrap_or(false));

    div()
        .id("feed")
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .child(
            gpui_adaptive::prim::list_scrollbar(div().size_full().child(rows), &list_state, cx),
        )
        .when(chat.loading_older.get(), |feed| {
            feed.child(
                div()
                    .absolute()
                    .top_2()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(spinner()),
            )
        })
        .when(scrolled_up, |feed| {
            feed.child(
                div()
                    .absolute()
                    .right_4()
                    .bottom_3()
                    .child(
                        Command::new("jump-to-latest", "Jump to latest")
                            .icon(Lucide::ArrowDown)
                            .on_trigger(ui.run(|ui, cx| {
                                // The newest message is not in a block that
                                // stops short of it: it has to be fetched.
                                if ui.chat.behind.get() {
                                    ui.return_to_present(cx);
                                    return;
                                }
                                ui.chat.scroll_to_newest("jump-button");
                                crate::ui::refresh(cx);
                            }))
                            .as_icon_button(),
                    ),
            )
        })
        .when_some(
            completion_list(ui, crate::ui::autocomplete::Composer::Channel, cx),
            |feed, list| feed.child(list),
        )
        .into_any_element()
}
