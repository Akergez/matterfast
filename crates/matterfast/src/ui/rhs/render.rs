use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
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

    let theme = cx.theme();
    let mut header = h_flex()
        .flex_none()
        .h(px(48.))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(theme.border)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .px_1()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                .when(!subtitle.is_empty(), |column| {
                    column.child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(subtitle),
                    )
                }),
        );

    // Following a thread is how you keep getting told about it after you stop
    // being mentioned in it, so it belongs on the thread itself.
    if matches!(mode, PanelMode::Thread(_)) {
        let following = panel.following.get();
        header = header.child(
            Button::new("follow")
                .icon(if following {
                    Lucide::BellRing
                } else {
                    Lucide::Bell
                })
                .small()
                .tooltip(if following {
                    "Following this thread"
                } else {
                    "Follow this thread"
                })
                .when(following, |button| button.primary())
                .when(!following, |button| button.ghost())
                .on_click(ui.click(move |ui, cx| {
                    ui.dispatch(Action::FollowThread(!following), cx)
                })),
        );
    }
    header = header.child(
        kit::icon_button("close-panel", Lucide::X, "Close panel")
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::CloseRightPanel, cx))),
    );

    let body = match mode {
        PanelMode::Thread(_) => thread_view(ui, cx),
        PanelMode::Search(_) => search_view(ui, cx),
        PanelMode::Hidden => return None,
    };

    Some(
        v_flex()
            .id("right-panel")
            .size_full()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(header)
            .child(body)
            .into_any_element(),
    )
}
