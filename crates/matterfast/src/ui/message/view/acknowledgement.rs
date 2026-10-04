use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{h_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App};
use mattermost_api::models::Post;

use crate::state::AppState;
use crate::ui::message::PostAction;
use crate::ui::{kit, Action, Ui};

/// The "please confirm you have read this" row on a priority message. Shown
/// as a button until you press it, then as who has.
pub(super) fn acknowledgement(ui: &Rc<Ui>, post: &Post, st: &AppState, cx: &App) -> AnyElement {
    let acks = post
        .metadata
        .as_ref()
        .map(|m| m.acknowledgements.as_slice())
        .unwrap_or_default();
    let mine = acks.iter().any(|a| a.user_id == st.me.id);

    let button = Button::new("acknowledge")
        .label(if mine { "Acknowledged" } else { "Acknowledge" })
        .small()
        .when(mine, |button| button.success())
        .when(!mine, |button| button.primary())
        .on_click(ui.click({
            let post_id = post.id.clone();
            move |ui, cx| {
                ui.dispatch(
                    Action::Post(
                        post_id.clone(),
                        if mine {
                            PostAction::Unacknowledge
                        } else {
                            PostAction::Acknowledge
                        },
                    ),
                    cx,
                )
            }
        }));

    let mut row = h_flex().gap_2().items_center().mt_1().child(button);
    if !acks.is_empty() {
        let names: Vec<String> = acks
            .iter()
            .filter_map(|a| st.users.get(&a.user_id))
            .map(|u| st.display_name(u))
            .collect();
        row = row.child(kit::with_tooltip(
            "acknowledged",
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{} acknowledged", acks.len())),
            names.join(", "),
        ));
    }
    row.into_any_element()
}
