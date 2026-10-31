use gpui_adaptive::prim::row;
use gpui_adaptive::ActiveColors as _;
use gpui_kit::prelude::*;
use gpui_kit::{div, App, SharedString};

/// A strip across the pane that says something is true until it stops being
/// true: the socket is down, a call is running, a message is being edited.
pub(super) fn banner(
    text: impl Into<SharedString>,
    color: gpui_kit::Hsla,
    cx: &App,
) -> gpui_kit::Div {
    row()
        .flex_none()
        .w_full()
        .px_3()
        .py_1p5()
        .gap_2()
        .items_center()
        .text_sm()
        .bg(color.opacity(0.14))
        .border_b_1()
        .border_color(cx.colors().border)
        .child(div().flex_1().min_w_0().child(text.into()))
}
