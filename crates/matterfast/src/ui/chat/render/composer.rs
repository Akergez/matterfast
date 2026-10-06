use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{h_flex, Sizable, Size};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App};

use super::completion_list::completion_keys;
use super::paste_image::paste_image;
use crate::ui::autocomplete::Composer;
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
            completion_keys(ui, Composer::Channel, div().flex_1().min_w_0()).child(
                Textarea::new(&state).on_paste({
                    let ui = ui.clone();
                    move |item, window, cx| {
                        paste_image(&ui, item, cx)
                            || crate::ui::paste_link::pasted(
                                &ui,
                                Composer::Channel,
                                item,
                                window,
                                cx,
                            )
                    }
                }),
            ),
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
