use std::rc::Rc;

use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App};

use super::inbox_row_view::inbox_row_view;
use super::inbox_tab::InboxTab;
use crate::ui::kit::{self, Lucide};
use crate::ui::Ui;

pub(super) fn inbox_view(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let panel = &ui.right;
    let inbox = panel.inbox.borrow();
    let tab = panel.tab.get();
    let (rows, empty) = match tab {
        InboxTab::Mentions => (
            &inbox.mentions,
            (
                Lucide::AtSign,
                "No recent mentions",
                "Messages that name you show up here.",
            ),
        ),
        InboxTab::Threads => (
            &inbox.threads,
            (
                Lucide::MessagesSquare,
                "No threads yet",
                "Threads you follow appear here.",
            ),
        ),
        InboxTab::Saved => (
            &inbox.saved,
            (
                Lucide::Bookmark,
                "Nothing saved",
                "Save a message from its menu and it waits here.",
            ),
        ),
    };

    let tabs = TabBar::new("inbox-tabs")
        .segmented()
        .selected_index(match tab {
            InboxTab::Mentions => 0,
            InboxTab::Threads => 1,
            InboxTab::Saved => 2,
        })
        .child(Tab::new().label("Mentions"))
        .child(Tab::new().label("Threads"))
        .child(Tab::new().label("Saved"))
        .on_click({
            let ui = ui.clone();
            move |index: &usize, _, cx| {
                ui.right.tab.set(match index {
                    1 => InboxTab::Threads,
                    2 => InboxTab::Saved,
                    _ => InboxTab::Mentions,
                });
                cx.refresh_windows();
            }
        });

    let body: AnyElement = if rows.is_empty() {
        kit::empty_state(empty.0, empty.1, empty.2, cx)
    } else {
        let mut column = v_flex().id("inbox-rows").size_full().p_1p5().gap_0p5();
        for (index, row) in rows.iter().enumerate() {
            column = column.child(inbox_row_view(ui, index, row, cx));
        }
        column.overflow_y_scroll().into_any_element()
    };

    v_flex()
        .flex_1()
        .min_h_0()
        .child(h_flex().flex_none().justify_center().py_2().child(tabs))
        .child(div().flex_1().min_h_0().child(body))
        .into_any_element()
}
