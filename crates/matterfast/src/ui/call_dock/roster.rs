use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, ElementId};

use super::host_action::HostAction;
use super::host_controls::host_controls;
use super::host_icon::host_icon;
use crate::ui::kit::{self, Lucide};
use crate::ui::{Action, Ui};

/// The participant list: face, name, and whatever is true of them right now —
/// talking, hand up, muted.
pub(super) fn roster(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let st = ui.state.borrow();
    let Some(call) = st.call.as_ref() else {
        return div().into_any_element();
    };
    let theme = cx.theme();
    let i_am_host = call.host_id == st.me.id;
    let people = st
        .active_calls
        .get(&call.channel_id)
        .cloned()
        .unwrap_or_default();

    let mut list = v_flex().w(px(280.)).gap_0p5();
    for user_id in &people {
        let name = st
            .users
            .get(user_id)
            .map(|u| u.display_name(st.teammate_name_display()))
            .unwrap_or_else(|| "Someone".to_string());
        let mut row = h_flex()
            .id(ElementId::Name(format!("participant-{user_id}").into()))
            .gap_2()
            .h(px(32.))
            .items_center()
            .child(kit::avatar(ui, user_id, &name, 24.))
            .child(div().flex_1().min_w_0().truncate().child(name));

        // A raised hand is a request and outranks the rest.
        if let Some(place) = call.hands.iter().position(|id| id == user_id) {
            row = row.child(kit::with_tooltip(
                "hand",
                format!("✋{}", place + 1),
                "Wants to speak",
            ));
        }
        if call.speaking.first() == Some(user_id) {
            row = row.child(kit::with_tooltip(
                "talking",
                div().text_color(theme.success).child(Lucide::Mic),
                "Talking",
            ));
        } else if call.muted_users.contains(user_id) {
            row = row.child(kit::with_tooltip(
                "muted",
                div()
                    .text_color(theme.muted_foreground)
                    .child(Lucide::MicOff),
                "Muted",
            ));
        }

        // Host controls, on the people they apply to. Shown only to the host,
        // and never against the host's own row: muting yourself is the button
        // already in the dock.
        if i_am_host && user_id != &st.me.id {
            if let Some(session_id) = call.sessions.get(user_id) {
                let controls = host_controls(
                    call.sharing.contains(user_id),
                    call.hands.iter().any(|id| id == user_id),
                );
                for (index, (action, tooltip)) in controls.into_iter().enumerate() {
                    // Most host routes address a session; making someone host
                    // addresses the person, since every session of theirs
                    // gains it at once.
                    let target = match action {
                        HostAction::MakeHost => user_id.clone(),
                        _ => session_id.clone(),
                    };
                    row = row.child(
                        kit::icon_button(("host", index), host_icon(action), tooltip)
                            .xsmall()
                            .on_click(ui.click(move |ui, cx| {
                                ui.dispatch(Action::HostControl(target.clone(), action), cx)
                            })),
                    );
                }
            }
        }

        if call.sharing.contains(user_id) {
            row = row.child(kit::with_tooltip(
                "sharing",
                div().text_color(theme.primary).child(Lucide::Monitor),
                "Sharing a screen",
            ));
        }
        list = list.child(row);
    }
    div()
        .id("roster")
        .max_h(px(280.))
        .overflow_y_scroll()
        .child(list)
        .into_any_element()
}
