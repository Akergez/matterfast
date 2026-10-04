use gpui_kit::component::{v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{px, App};

/// The bordered card a quoted message or a link preview sits in.
pub(super) fn preview_card(cx: &App) -> gpui_kit::Div {
    v_flex()
        .mt_1()
        .p_2()
        .gap_0p5()
        .max_w(px(520.))
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(0.4))
}
