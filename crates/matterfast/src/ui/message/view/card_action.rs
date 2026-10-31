use gpui_adaptive::prim::testable;
use gpui_adaptive::{ActiveColors as _, ActiveScale as _, Command, Menu};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, ElementId, Hsla};
use mattermost_api::models::AttachmentAction;

use crate::ui::{Action, Ui};

/// A CSS-style colour — "#2eb886", or one of the three words Slack-compatible
/// integrations use — as something to paint with.
pub(super) fn card_color(color: &str, cx: &App) -> Option<gpui_kit::Hsla> {
    match color {
        "good" => Some(cx.colors().success),
        "warning" => Some(cx.colors().warning),
        "danger" => Some(cx.colors().danger),
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
        return action_button(id, action, move |_, cx| press(String::new(), cx), cx);
    }

    // A menu starts on its default, and otherwise on its own name, which is
    // what integrations use as the placeholder.
    let label = action
        .options
        .iter()
        .find(|option| !action.default_option.is_empty() && option.value == action.default_option)
        .map(|option| option.text.clone())
        .unwrap_or_else(|| action.name.clone());
    let button = Command::new(id, label).disabled(action.disabled);

    // The server's own people or channels: too many for a menu, so a list.
    if matches!(action.data_source.as_str(), "users" | "channels") {
        let ui = ui.clone();
        let (title, channels) = (action.name.clone(), action.data_source == "channels");
        let press = Rc::new(press);
        return button
            .on_trigger(move |_, cx| {
                let (title, press) = (title.clone(), press.clone());
                ui.later(cx, move |ui, cx| {
                    ui.pick_from_directory(&title, channels, move |value, cx| press(value, cx), cx)
                });
            })
            .as_button()
            .into_any_element();
    }

    let current = action.default_option.clone();
    let press = Rc::new(press);
    Menu::new(button)
                            .choice()
        .title(action.name.clone())
        .commands(action.options.iter().enumerate().map(|(index, option)| {
            let (press, value) = (press.clone(), option.value.clone());
            Command::new(("card-option", index), option.text.clone())
                .checked(!current.is_empty() && option.value == current)
                .on_trigger(move |_, cx| press(value.clone(), cx))
        }))
        .into_any_element()
}

/// A plain button, styled by the words the server uses. Anything else is a
/// colour of the integration's own, drawn the way the web client draws it:
/// the colour as text on a faint wash of itself, which stays readable
/// whatever was chosen.
fn action_button(
    id: ElementId,
    action: &AttachmentAction,
    press: impl Fn(&mut gpui_kit::Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let command = Command::new(id.clone(), action.name.clone()).disabled(action.disabled);
    let colors = cx.colors();
    let tint = match action.style.as_str() {
        "primary" => return command.primary().on_trigger(press).as_button().into_any_element(),
        "danger" => return command.destructive().on_trigger(press).as_button().into_any_element(),
        "success" | "good" => Some(colors.success),
        "warning" => Some(colors.warning),
        other => card_color(other, cx),
    };
    match tint {
        Some(color) => tinted(id, action.name.clone(), color, action.disabled, press, cx),
        None => command.on_trigger(press).as_button().into_any_element(),
    }
}

/// The integration's colour as text on a wash of itself.
fn tinted(
    id: ElementId,
    label: String,
    color: Hsla,
    disabled: bool,
    press: impl Fn(&mut gpui_kit::Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let scale = cx.scale();
    // Its own hover and press, in its colour: not a pressable's.
    let button = gpui_adaptive::prim::row()
        .id(id)
        .cursor_pointer()
        .h(scale.control)
        .px_3()
        .justify_center()
        .rounded(scale.radius_control)
        .text_size(scale.small)
        .text_color(color)
        .bg(color.opacity(0.1))
        .hover(move |style| style.bg(color.opacity(0.2)))
        .active(move |style| style.bg(color.opacity(0.3)))
        .child(label);
    if disabled {
        return button.opacity(0.5).cursor_default().into_any_element();
    }
    testable(button.on_click(move |_, window, cx| press(window, cx)))
}
