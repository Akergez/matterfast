use std::rc::Rc;

use gpui_kit::component::input::{Enter, Escape, IndentInline, MoveDown, MoveUp};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, Div, SharedString};

use crate::ui::autocomplete::Composer;
use crate::ui::message;
use crate::ui::{Action, Ui};

/// Gives the completion list of a box first refusal on the keys that move
/// through it: they do that while it is open, and only reach the text when
/// it is not. `holder` is what the text box is put in.
pub(crate) fn completion_keys(ui: &Rc<Ui>, which: Composer, holder: Div) -> Div {
    let open = move |ui: &Rc<Ui>| ui.completions(which).borrow().is_open();
    holder
        .capture_action({
            let ui = ui.clone();
            move |_: &MoveUp, _, cx| {
                if open(&ui) {
                    ui.completions(which).borrow_mut().step(-1);
                    cx.stop_propagation();
                    crate::ui::refresh(cx);
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &MoveDown, _, cx| {
                if open(&ui) {
                    ui.completions(which).borrow_mut().step(1);
                    cx.stop_propagation();
                    crate::ui::refresh(cx);
                }
            }
        })
        // Enter picks from the list while it is up. Taken here, before the
        // text box sees the key: left to arrive as the box's own "submitted",
        // it had already been treated as typing.
        .capture_action({
            let ui = ui.clone();
            move |enter: &Enter, _, cx| {
                if open(&ui) && !enter.shift {
                    cx.stop_propagation();
                    ui.later(cx, move |ui, cx| {
                        ui.accept_completion(which, cx);
                    });
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &IndentInline, _, cx| {
                if open(&ui) {
                    cx.stop_propagation();
                    ui.later(cx, move |ui, cx| {
                        ui.accept_completion(which, cx);
                    });
                }
            }
        })
        .capture_action({
            let ui = ui.clone();
            move |_: &Escape, _, cx| {
                if open(&ui) {
                    ui.completions(which).borrow_mut().close();
                    cx.stop_propagation();
                    crate::ui::refresh(cx);
                    // Whatever is in flight for the closed query must not
                    // reopen the list.
                    ui.dispatch(Action::Complete(None), cx);
                }
            }
        })
}

/// The completion list of a box, floating over the bottom of what is above
/// it: the feed for the conversation's, the replies for a thread's.
pub(crate) fn completion_list(ui: &Rc<Ui>, which: Composer, cx: &App) -> Option<AnyElement> {
    let completions = ui.completions(which).borrow();
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
        // A thread's column can be narrower than the list would like to be.
        .max_w_full()
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
            ui.completions(which).borrow_mut().selected = index;
            ui.accept_completion(which, cx);
        })));
    }
    Some(rows.into_any_element())
}
