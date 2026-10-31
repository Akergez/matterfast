use std::rc::Rc;

use gpui_adaptive::prim::row;
use gpui_adaptive::{Command, Menu, TextField};
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
    // The button carries the current choice: a priority you set and cannot
    // see is one you will send by accident.
    let button = Command::new("priority", "Message priority").icon(Lucide::CircleAlert);
    let button = match priority.as_ref() {
        "urgent" => button.destructive(),
        "important" => button.primary(),
        _ => button,
    };
    let priorities = [("Standard", ""), ("Important", "important"), ("Urgent", "urgent")].map(|(label, value)| {
        Command::new(label, label)
            .checked(*chat.priority.borrow() == value)
            .on_trigger(ui.run(move |ui, cx| ui.chat.set_priority(value, cx)))
    });

    row()
        .flex_none()
        .w_full()
        .gap_2()
        .px_3()
        .pt_1()
        .pb_3()
        .items_center()
        .child(
            kit::icon_button("attach", Lucide::Paperclip, "Attach a file")
                .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::PickAttachment, cx)))
                .as_icon_button(),
        )
        .child(Menu::new(button).title("Message priority").commands(priorities))
        .child(
            kit::icon_button("schedule", Lucide::AlarmClock, "Send later")
                .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::ScheduleMessage, cx)))
                .as_icon_button(),
        )
        .child(
            div().flex_1().min_w_0().child(
                TextField::new(&state).on_key(completion_keys(ui, Composer::Channel)).on_paste({
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
            Command::new("send", "Send  (Enter)")
                .icon(Lucide::SendHorizontal)
                .primary()
                .on_trigger(ui.run(|ui, cx| ui.chat.submit(ui, cx)))
                .as_icon_button(),
        )
        .into_any_element()
}
