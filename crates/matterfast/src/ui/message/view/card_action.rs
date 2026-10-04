use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Disableable, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, ElementId};
use mattermost_api::models::AttachmentAction;

use crate::ui::{Action, Ui};

/// A CSS-style colour — "#2eb886", or one of the three words Slack-compatible
/// integrations use — as something to paint with.
pub(super) fn card_color(color: &str, cx: &App) -> Option<gpui_kit::Hsla> {
    match color {
        "good" => Some(cx.theme().success),
        "warning" => Some(cx.theme().warning),
        "danger" => Some(cx.theme().danger),
        hex => {
            let hex = hex.strip_prefix('#')?;
            if hex.len() != 6 {
                return None;
            }
            let value = u32::from_str_radix(hex, 16).ok()?;
            Some(gpui_kit::rgb(value).into())
        }
    }
}

/// Something to press on a card: a button, or a menu that sends what was
/// picked. Neither changes anything here — the integration answers by editing
/// the post, and that edit is what the person sees.
pub(super) fn card_action(
    ui: &Rc<Ui>,
    post_id: &str,
    card: usize,
    action: &AttachmentAction,
    cx: &App,
) -> AnyElement {
    let id = ElementId::Name(format!("card-{card}-action-{}", action.id).into());
    let button = Button::new(id).small().disabled(action.disabled);
    let press = {
        let (ui, post_id) = (ui.clone(), post_id.to_string());
        let (action_id, cookie) = (action.id.clone(), action.cookie.clone());
        move |selected: String, cx: &mut App| {
            ui.dispatch(
                Action::CardAction {
                    post_id: post_id.clone(),
                    action_id: action_id.clone(),
                    selected,
                    cookie: cookie.clone(),
                },
                cx,
            )
        }
    };

    if !action.is_select() {
        return action_button(button, action, cx)
            .on_click(move |_, _, cx| press(String::new(), cx))
            .into_any_element();
    }

    // A menu starts on its default, and otherwise on its own name, which is
    // what integrations use as the placeholder.
    let label = action
        .options
        .iter()
        .find(|option| !action.default_option.is_empty() && option.value == action.default_option)
        .map(|option| option.text.clone())
        .unwrap_or_else(|| action.name.clone());
    let button = button.label(label).outline().dropdown_caret(true);

    // The server's own people or channels: too many for a menu, so a list.
    if matches!(action.data_source.as_str(), "users" | "channels") {
        let ui = ui.clone();
        let (title, channels) = (action.name.clone(), action.data_source == "channels");
        let press = Rc::new(press);
        return button
            .on_click(move |_, _, cx| {
                let (title, press) = (title.clone(), press.clone());
                ui.later(cx, move |ui, cx| {
                    ui.pick_from_directory(&title, channels, move |value, cx| press(value, cx), cx)
                });
            })
            .into_any_element();
    }

    let options = action.options.clone();
    let current = action.default_option.clone();
    let press = Rc::new(press);
    button
        .dropdown_menu(move |mut menu, _, _| {
            for option in &options {
                let (press, value) = (press.clone(), option.value.clone());
                menu = menu.item(
                    PopupMenuItem::new(option.text.clone())
                        .checked(!current.is_empty() && option.value == current)
                        .on_click(move |_, _, cx| press(value.clone(), cx)),
                );
            }
            menu
        })
        .into_any_element()
}

/// A plain button, styled by the words the server uses. Anything else is a
/// colour of the integration's own, drawn the way the web client draws it:
/// the colour as text on a faint wash of itself, which stays readable
/// whatever was chosen.
fn action_button(button: Button, action: &AttachmentAction, cx: &App) -> Button {
    let button = button.label(action.name.clone());
    match action.style.as_str() {
        "primary" => button.primary(),
        "success" | "good" => button.success(),
        "warning" => button.warning(),
        "danger" => button.danger(),
        other => match card_color(other, cx) {
            Some(color) => button.custom(
                ButtonCustomVariant::new(cx)
                    .color(color.opacity(0.1))
                    .foreground(color)
                    .hover(color.opacity(0.2))
                    .active(color.opacity(0.3)),
            ),
            None => button.outline(),
        },
    }
}
