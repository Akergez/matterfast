use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Enter, Escape, IndentInline, MoveDown, MoveUp, Textarea};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{h_flex, Sizable, Size};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App};

use super::paste_image::paste_image;
use crate::ui::kit::{self, Lucide};
use crate::ui::{Action, Ui};

/// The composer row.
pub(super) fn composer(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let chat = &ui.chat;
    let Some(state) = chat.composer.borrow().clone() else {
        return div().into_any_element();
    };
    let priority = chat.priority(cx);
    let priority_ui = ui.clone();

    h_flex()
        .flex_none()
        .w_full()
        .gap_2()
        .px_3()
        .pt_1()
        .pb_3()
        .items_center()
        .child(
            kit::icon_button("attach", Lucide::Paperclip, "Attach a file")
                .with_size(Size::Medium)
                .on_click(ui.click(|ui, cx| ui.dispatch(Action::PickAttachment, cx))),
        )
        .child(
            // The button carries the current choice: a priority you set and
            // cannot see is one you will send by accident.
            Button::new("priority")
                .icon(Lucide::CircleAlert)
                .tooltip("Message priority")
                .when(priority == "urgent", |button| button.danger())
                .when(priority == "important", |button| button.primary())
                .when(priority.is_empty(), |button| button.ghost())
                .dropdown_menu(move |mut menu, _, _| {
                    for (label, value) in [
                        ("Standard", ""),
                        ("Important", "important"),
                        ("Urgent", "urgent"),
                    ] {
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(*priority_ui.chat.priority.borrow() == value)
                                .on_click(priority_ui.click(move |ui, cx| {
                                    ui.chat.set_priority(value, cx)
                                })),
                        );
                    }
                    menu
                }),
        )
        .child(
            kit::icon_button("schedule", Lucide::AlarmClock, "Send later")
                .with_size(Size::Medium)
                .on_click(ui.click(|ui, cx| ui.dispatch(Action::ScheduleMessage, cx))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                // The completion list has first refusal on these keys: they
                // move through it while it is open, and only reach the text
                // when it is not.
                .capture_action({
                    let ui = ui.clone();
                    move |_: &MoveUp, _, cx| {
                        if ui.chat.completing() {
                            ui.chat.completions.borrow_mut().step(-1);
                            cx.stop_propagation();
                            crate::ui::refresh(cx);
                        }
                    }
                })
                .capture_action({
                    let ui = ui.clone();
                    move |_: &MoveDown, _, cx| {
                        if ui.chat.completing() {
                            ui.chat.completions.borrow_mut().step(1);
                            cx.stop_propagation();
                            crate::ui::refresh(cx);
                        }
                    }
                })
                // Enter picks from the list while it is up. Taken here, before
                // the text box sees the key: left to arrive as the box's own
                // "submitted", it had already been treated as typing.
                .capture_action({
                    let ui = ui.clone();
                    move |enter: &Enter, _, cx| {
                        if ui.chat.completing() && !enter.shift {
                            cx.stop_propagation();
                            ui.later(cx, |ui, cx| {
                                ui.chat.accept_completion(ui, cx);
                            });
                        }
                    }
                })
                .capture_action({
                    let ui = ui.clone();
                    move |_: &IndentInline, _, cx| {
                        if ui.chat.completing() {
                            cx.stop_propagation();
                            ui.later(cx, |ui, cx| {
                                ui.chat.accept_completion(ui, cx);
                            });
                        }
                    }
                })
                .capture_action({
                    let ui = ui.clone();
                    move |_: &Escape, _, cx| {
                        if ui.chat.completing() {
                            ui.chat.completions.borrow_mut().close();
                            cx.stop_propagation();
                            crate::ui::refresh(cx);
                            // Whatever is in flight for the closed query must
                            // not reopen the list.
                            ui.dispatch(Action::Complete(None), cx);
                        }
                    }
                })
                .child(Textarea::new(&state).on_paste({
                    let ui = ui.clone();
                    move |item, _, cx| paste_image(&ui, item, cx)
                })),
        )
        .child(
            Button::new("send")
                .icon(Lucide::SendHorizontal)
                .primary()
                .tooltip("Send  (Enter)")
                .on_click(ui.click(|ui, cx| ui.chat.submit(ui, cx))),
        )
        .into_any_element()
}
