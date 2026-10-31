use gpui_adaptive::prim::{column, row};
use gpui_adaptive::{ActiveColors as _, Command};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, FontWeight};

use super::panel_mode::PanelMode;
use super::search_view::search_view;
use super::thread_view::thread_view;
use crate::ui::kit::{self, Lucide};
use crate::ui::{Action, Ui};

/// Draws the panel, or nothing when it is hidden.
pub fn render(ui: &Rc<Ui>, cx: &mut App) -> Option<AnyElement> {
    let panel = &ui.right;
    let mode = panel.mode(cx);
    let (title, subtitle) = match &mode {
        PanelMode::Hidden => return None,
        PanelMode::Thread(_) => ("Thread", panel.thread_channel.borrow().clone()),
        PanelMode::Search(terms) => ("Search", terms.clone()),
    };

    let colors = cx.colors();
    let mut header = row()
        .flex_none()
        .h(px(48.))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(colors.border)
        .child(
            column()
                .flex_1()
                .min_w_0()
                .px_1()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                .when(!subtitle.is_empty(), |column| {
                    column.child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(colors.muted)
                            .child(subtitle),
                    )
                }),
        );

    // Following a thread is how you keep getting told about it after you stop
    // being mentioned in it, so it belongs on the thread itself.
    if matches!(mode, PanelMode::Thread(_)) {
        let following = panel.following.get();
        header = header.child(
            Command::new("follow", if following {
                    "Following this thread"
                } else {
                    "Follow this thread"
                })
                .icon(if following {
                    Lucide::BellRing
                } else {
                    Lucide::Bell
                })
                .checked(following)
                .on_trigger(ui.run(move |ui, cx| {
                    ui.dispatch(Action::FollowThread(!following), cx)
                }))
                .as_icon_button(),
        );
    }
    header = header.child(
        kit::icon_button("close-panel", Lucide::X, "Close panel")
            .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::CloseRightPanel, cx)))
            .as_icon_button(),
    );

    let body = match mode {
        PanelMode::Thread(_) => thread_view(ui, cx),
        PanelMode::Search(_) => search_view(ui, cx),
        PanelMode::Hidden => return None,
    };

    Some(
        column()
            .id("right-panel")
            .size_full()
            .bg(cx.colors().surface)
            .border_l_1()
            .border_color(cx.colors().border)
            .child(header)
            .child(body)
            .into_any_element(),
    )
}
