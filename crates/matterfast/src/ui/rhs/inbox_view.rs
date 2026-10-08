use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{h_flex, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, list, AnyElement, App};

use super::inbox::Entry;
use super::inbox_row_view::inbox_row_view;
use crate::ui::kit::{self, Lucide};
use crate::ui::{Action, Ui};

/// The inbox: everything the reader is being spoken to in, or follows, as
/// one list with the newest first — threads, messages that name them, what
/// they saved, and the conversations they follow as a whole. It is drawn in
/// the column of conversations, as the first of its tabs; what it lists is
/// kept here with the thread it opens ([`build_inbox`](super::build_inbox)).
///
/// Only the rows in view are built: see `RightPanel::inbox_list`.
pub(crate) fn inbox_view(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    if ui.right.inbox.borrow().entries.is_empty() {
        return div()
            .flex_1()
            .min_h_0()
            .child(kit::empty_state(
                Lucide::Inbox,
                "Nothing waiting",
                "Threads you are in, messages that name you and the channels you follow show up here.",
                cx,
            ))
            .into_any_element();
    }

    let row_ui = ui.clone();
    let rows = list(ui.right.inbox_list.clone(), move |index, _, cx| {
        let began = std::time::Instant::now();
        let inbox = row_ui.right.inbox.borrow();
        let row = match inbox.entries.get(index) {
            Some(Entry::Post(row)) => Some(inbox_row_view(&row_ui, index, row, cx)),
            Some(Entry::Chat(id)) => {
                let st = row_ui.state.borrow();
                st.channels
                    .get(id)
                    .map(|channel| {
                        let fit = crate::ui::sidebar::Fit::INBOX;
                        crate::ui::sidebar::channel_row(&row_ui, channel, &st, fit, cx)
                    })
            }
            // Past the entries is the one row that asks for more of them:
            // the server keeps the followed threads and hands them over a
            // page at a time, newest first, and the rest are a press away
            // rather than fetched in case somebody scrolls that far.
            None if inbox.more && index == inbox.entries.len() => Some(
                h_flex()
                    .justify_center()
                    .py_2()
                    .child(
                        Button::new("older-threads")
                            .small()
                            .ghost()
                            .label("Older threads")
                            .on_click(
                                row_ui.click(|ui, cx| ui.dispatch(Action::OlderThreads, cx)),
                            ),
                    )
                    .into_any_element(),
            ),
            None => None,
        };
        crate::ui::frame_log::row(crate::ui::Part::Sidebar, began);
        // A row the inbox no longer has is gone from the list one frame
        // later.
        div()
            .w_full()
            .children(row)
            .into_any_element()
    })
    .size_full();

    div()
        .id("inbox-rows")
        .flex_1()
        .min_h_0()
        .px_1p5()
        .pt_1()
        .child(rows)
        .into_any_element()
}
