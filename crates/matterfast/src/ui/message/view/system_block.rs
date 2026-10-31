use gpui_adaptive::prim::column;
use gpui_adaptive::ActiveColors as _;
use gpui_kit::prelude::*;
use gpui_kit::{px, AnyElement, App, ElementId, SharedString};

use super::markdown_view::markdown;

/// A run of system lines: joins, leaves, a header change.
pub fn system_block(index_key: &str, lines: &[SharedString], cx: &App) -> AnyElement {
    let mut block = column()
        .id(ElementId::Name(format!("system-{index_key}").into()))
        .w_full()
        .gap_0p5()
        // Under the avatar column, so it reads as part of the conversation.
        .pl(px(62.))
        .pr_3()
        .py_0p5()
        .text_sm()
        .text_color(cx.colors().muted);
    for (index, line) in lines.iter().enumerate() {
        block = block.child(markdown(("line", index), line.clone()));
    }
    block.into_any_element()
}
