use std::rc::Rc;

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, FontWeight};

use crate::ui::kit::{self, Lucide};
use crate::ui::message::{self, RowOptions};
use crate::ui::{Action, Ui};

/// Search results, newest first, each one a jump into its channel.
pub(super) fn search_view(ui: &Rc<Ui>, cx: &mut App) -> AnyElement {
    let panel = &ui.right;
    let rows = panel.search.borrow().clone();
    let searching = panel.searching.get();
    if searching && rows.is_empty() {
        return div()
            .flex_1()
            .flex()
            .justify_center()
            .pt_6()
            .child(Spinner::new())
            .into_any_element();
    }
    if rows.is_empty() {
        return div()
            .flex_1()
            .min_h_0()
            .child(kit::empty_state(
                Lucide::Search,
                "No matches",
                "Nothing here matched that search.",
                cx,
            ))
            .into_any_element();
    }

    // The hits come a page at a time. Being within a screenful of the end of
    // what is here is what asks for the next one. How far the end is comes
    // from the frame before, so it is only believed when that frame drew
    // these same rows: on the frame a page arrives it still describes the
    // shorter list, and would ask for the page after as well.
    let scroll = &panel.search_scroll;
    let measured = panel.search_drawn.replace(rows.len()) == rows.len();
    let left = scroll.max_offset().y + scroll.offset().y;
    if panel.search_more.get() && !searching && measured && left < px(400.) {
        ui.dispatch(Action::SearchMore, cx);
    }

    let mut column = v_flex()
        .id("search-rows")
        .flex_1()
        .min_h_0()
        .py_1p5()
        .track_scroll(scroll);
    for (index, row) in rows.iter().enumerate() {
        let channel_id = row.post.channel_id.clone();
        let post_id = row.post.id.clone();
        column = column.child(
            v_flex()
                .id(("hit", index))
                .w_full()
                .child(
                    // Which channel a hit came from is also the obvious place
                    // to press to go there.
                    div()
                        .id("go")
                        .ml_3()
                        .mt_2()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().muted_foreground)
                        .cursor_pointer()
                        .hover(|style| style.underline())
                        .child(format!("{}  ›", row.channel))
                        .on_click(ui.click(move |ui, cx| {
                            ui.dispatch(
                                Action::JumpToPost(channel_id.clone(), post_id.clone()),
                                cx,
                            )
                        })),
                )
                .child(message::row(
                    ui,
                    &row.post,
                    &row.body,
                    RowOptions {
                        grouped: false,
                        show_thread_footer: false,
                        highlight: false,
                    },
                    cx,
                )),
        );
    }
    if searching {
        column = column.child(
            div()
                .flex()
                .justify_center()
                .py_3()
                .child(Spinner::new().small()),
        );
    }
    column.overflow_y_scroll().into_any_element()
}
