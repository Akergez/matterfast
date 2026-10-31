use gpui_adaptive::prim::{column, row};
use gpui_adaptive::ActiveColors as _;
use std::rc::Rc;

use gpui_adaptive::prim::Scrolls as _;
use gpui_adaptive::FieldKey;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, Window};

use crate::ui::autocomplete::Composer;
use crate::ui::{kit, message};
use crate::ui::{Action, Ui};

/// Gives the completion list of a box first refusal on the keys that move
/// through it: they do that while it is open, and only reach the text when
/// it is not. For the box's [`TextField::on_key`].
pub(crate) fn completion_keys(
    ui: &Rc<Ui>,
    which: Composer,
) -> impl Fn(FieldKey, &mut Window, &mut App) -> bool + 'static {
    let ui = ui.clone();
    move |key, _, cx| {
        if !ui.completions(which).borrow().is_open() {
            return false;
        }
        match key {
            FieldKey::Up | FieldKey::Down => {
                ui.completions(which).borrow_mut().step(if key == FieldKey::Up { -1 } else { 1 });
                crate::ui::refresh(cx);
            }
            // Enter and Tab pick from the list while it is up.
            FieldKey::Enter | FieldKey::Tab => {
                ui.later(cx, move |ui, cx| {
                    ui.accept_completion(which, cx);
                });
            }
            FieldKey::Escape => {
                ui.completions(which).borrow_mut().close();
                crate::ui::refresh(cx);
                // Whatever is in flight for the closed query must not
                // reopen the list.
                ui.dispatch(Action::Complete(None), cx);
            }
        }
        true
    }
}

/// The completion list of a box, floating over the bottom of what is above
/// it: the feed for the conversation's, the replies for a thread's.
pub(crate) fn completion_list(ui: &Rc<Ui>, which: Composer, cx: &App) -> Option<AnyElement> {
    let completions = ui.completions(which).borrow();
    if !completions.is_open() {
        return None;
    }
    let colors = cx.colors();
    let mut rows = column()
        .id("completions")
        .absolute()
        .left_3()
        .bottom_1()
        .w(px(340.))
        // A thread's column can be narrower than the list would like to be.
        .max_w_full()
        .max_h(px(260.))
        .scrolls_y()
        .p_1()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.raised)
        .shadow_md();
    for (index, item) in completions.items.iter().enumerate() {
        let selected = index == completions.selected;
        let mut row = row()
            .id(("candidate", index))
            .gap_2()
            .px_2()
            .h(px(32.))
            .items_center()
            .rounded_sm()
            .cursor_pointer()
            .when(selected, |row| row.bg(colors.hover))
            .hover(|style| style.bg(colors.hover));

        // A mention is a person and gets a face — initials until the picture
        // lands, as everywhere else, so the rows stay aligned. An emoji is
        // its own picture and gets none.
        if item.insert.starts_with('@') {
            let avatar = gpui_adaptive::Avatar::new(("candidate-face", index), kit::initials(&item.primary))
                .size(gpui_kit::rems(1.5));
            row = row.child(match &item.image {
                Some(picture) => avatar.picture(picture.clone()),
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
                    .text_color(colors.muted)
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
