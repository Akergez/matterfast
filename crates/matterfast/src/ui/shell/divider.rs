use std::rc::Rc;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, Context, Window};

use crate::ui::{Divider, Ui};

/// What follows the pointer while a divider is dragged: nothing. The column
/// moving is the feedback.
struct DividerGhost;

impl Render for DividerGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// The strip that is dragged to resize a side column. It lies over the border
/// between two panes, a few pixels to either side, and draws nothing until
/// the pointer is on it. A double click gives the column back its share.
pub(super) fn divider(ui: &Rc<Ui>, which: Divider, offset: f32, cx: &App) -> AnyElement {
    const GRIP: f32 = 7.0;
    let line = cx.theme().ring;
    let strip = div()
        .id(match which {
            Divider::Sidebar => "divider-sidebar",
            Divider::Panel => "divider-panel",
        })
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(GRIP))
        .flex()
        .justify_center()
        .cursor_col_resize()
        .occlude()
        .group("divider")
        .child(
            div()
                .w(px(2.))
                .h_full()
                .group_hover("divider", move |mark| mark.bg(line)),
        )
        .on_drag(which, |_, _, _, cx| cx.new(|_| DividerGhost))
        .on_click({
            let reset = ui.click(move |ui, cx| ui.widths.set(which, None, cx));
            move |click, window, cx| {
                if click.click_count() == 2 {
                    reset(click, window, cx);
                }
            }
        });
    match which {
        Divider::Sidebar => strip.left(px(offset - GRIP / 2.0)),
        Divider::Panel => strip.right(px(offset - GRIP / 2.0)),
    }
    .into_any_element()
}
