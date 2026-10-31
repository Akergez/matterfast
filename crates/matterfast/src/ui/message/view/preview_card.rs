use gpui_adaptive::prim::column;
use gpui_adaptive::ActiveColors as _;
use gpui_kit::prelude::*;
use gpui_kit::{px, App};

/// The bordered card a quoted message or a link preview sits in.
pub(super) fn preview_card(cx: &App) -> gpui_kit::Div {
    column()
        .mt_1()
        .p_2()
        .gap_0p5()
        .max_w(px(520.))
        .rounded_md()
        .border_1()
        .border_color(cx.colors().border)
        .bg(cx.colors().sunken.opacity(0.4))
}
