use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, SharedString};

use crate::ui::message;
use crate::ui::Ui;

/// The completion list, floating over the bottom of the feed just above the
/// composer.
pub(super) fn completion_list(ui: &Rc<Ui>, cx: &App) -> Option<AnyElement> {
    let completions = ui.chat.completions.borrow();
    if !completions.is_open() {
        return None;
    }
    let theme = cx.theme();
    let mut rows = v_flex()
        .id("completions")
        .absolute()
        .left_3()
        .bottom_1()
        .w(px(340.))
        .max_h(px(260.))
        .overflow_y_scroll()
        .p_1()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .shadow_md();
    for (index, item) in completions.items.iter().enumerate() {
        let selected = index == completions.selected;
        let mut row = h_flex()
            .id(("candidate", index))
            .gap_2()
            .px_2()
            .h(px(32.))
            .items_center()
            .rounded_sm()
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.accent))
            .hover(|style| style.bg(theme.list_hover));

        // A mention is a person and gets a face — initials until the picture
        // lands, as everywhere else, so the rows stay aligned. An emoji is
        // its own picture and gets none.
        if item.insert.starts_with('@') {
            let avatar = gpui_kit::component::avatar::Avatar::new()
                .name(SharedString::from(item.primary.clone()))
                .with_size(gpui_kit::component::Size::Size(px(24.)));
            row = row.child(match &item.image {
                Some(picture) => avatar.src(picture.clone()),
                None => avatar,
            });
        }
        if let Some(name) = &item.emoji {
            row = row.child(message::emoji_element(ui, name, 20.));
        }
        row = row.child(div().flex_none().truncate().child(item.primary.clone()));
        if !item.secondary.is_empty() {
            row = row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_right()
                    .text_color(theme.muted_foreground)
                    .child(item.secondary.clone()),
            );
        }
        rows = rows.child(row.on_click(ui.click(move |ui, cx| {
            ui.chat.completions.borrow_mut().selected = index;
            ui.chat.accept_completion(ui, cx);
        })));
    }
    Some(rows.into_any_element())
}
