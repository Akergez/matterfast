//! The profile card, the way Mattermost shows it when you click a name.

use std::rc::Rc;

use chrono::Utc;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, FontWeight};
use mattermost_api::models::{Presence, User};

use super::kit;
use super::{Action, Ui};
use crate::emoji;
use crate::timefmt::format_day;

/// Shows the card for `user_id`.
///
/// Answers `false` when we do not hold the user — the caller fetches them and
/// asks again, rather than this showing a card with nothing on it.
pub fn show(ui: &Rc<Ui>, user_id: &str, cx: &mut App) -> bool {
    if !ui.state.borrow().users.contains_key(user_id) {
        return false;
    }
    let user_id = user_id.to_string();
    let card_ui = ui.clone();
    ui.with_window(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, _, cx| {
            // Read on every draw: presence and the picture both change while
            // the card is up.
            dialog.w(px(320.)).child(card(&card_ui, &user_id, cx))
        });
    });
    true
}

fn detail_row(label: &'static str, value: String, cx: &App) -> gpui_kit::Div {
    h_flex()
        .gap_2()
        .text_xs()
        .child(
            div()
                .flex_none()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().flex_1().min_w_0().truncate().text_right().child(value))
}

fn card(ui: &Rc<Ui>, user_id: &str, cx: &App) -> gpui_kit::AnyElement {
    let st = ui.state.borrow();
    let Some(user) = st.users.get(user_id).cloned() else {
        return div().into_any_element();
    };
    let display = st.display_name(&user);
    let presence = st.presence(user_id);
    let is_me = user.id == st.me.id;
    let last_seen = st.last_seen.get(user_id).copied().filter(|at| *at > 0);
    drop(st);

    let muted = cx.theme().muted_foreground;
    let mut column = v_flex()
        .items_center()
        .gap_1()
        .child(kit::avatar(ui, &user.id, &display, 72.))
        .child(
            div()
                .mt_2()
                .text_lg()
                .text_center()
                .font_weight(FontWeight::SEMIBOLD)
                .child(display),
        )
        .child(
            div()
                .text_color(muted)
                .child(format!("@{}", user.username)),
        )
        // Presence, as a coloured dot plus a word — colour alone would not
        // survive a high-contrast theme or a colour-blind reader.
        .child(
            h_flex()
                .mt_1()
                .gap_1p5()
                .items_center()
                .child(
                    div()
                        .size(px(9.))
                        .rounded_full()
                        .bg(kit::presence_color(presence, cx)),
                )
                .child(presence_label(presence)),
        );

    // When somebody was last seen. Only for people who are not online now —
    // "last seen 26 August" under a green dot would be nonsense.
    if presence != Presence::Online {
        if let Some(seen) = last_seen {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("Last seen {}", format_day(seen))),
            );
        }
    }

    if let Some(status) = user
        .custom_status()
        .filter(super::message::status_is_live)
    {
        // The card has room, so the expiry goes in the text rather than
        // hiding in a tooltip on a tooltip.
        column = column.child(
            div().mt_1().text_center().child(
                format!(
                    "{} {}",
                    emoji::label(&status.emoji),
                    super::message::custom_status_tooltip(&status)
                )
                .trim()
                .to_string(),
            ),
        );
    }

    if !user.position.is_empty() {
        column = column.child(
            div()
                .mt_1()
                .text_center()
                .text_color(muted)
                .child(user.position.clone()),
        );
    }

    let mut details = v_flex().w_full().mt_3().gap_1();
    if let Some(local) = local_time(&user) {
        details = details.child(detail_row("Local time", local, cx));
    }
    if !user.email.is_empty() {
        details = details.child(detail_row("Email", user.email.clone(), cx));
    }
    column = column.child(details);

    if !is_me {
        let id = user.id.clone();
        column = column.child(
            Button::new("send-message")
                .label("Send message")
                .primary()
                .mt_3()
                .on_click({
                    let ui = ui.clone();
                    move |_, window, cx| {
                        window.close_dialog(cx);
                        ui.dispatch(Action::OpenDirectMessage(id.clone()), cx);
                    }
                }),
        );
    }
    column.into_any_element()
}

/// The user's wall-clock time, which is the detail that actually changes how
/// you treat a message.
///
/// `timezone` is a `map[string]string`: `useAutomaticTimezone` is the *string*
/// `"true"`/`"false"`, and the effective zone is whichever of the two fields
/// that flag selects.
fn local_time(user: &User) -> Option<String> {
    let automatic = user
        .timezone
        .get("useAutomaticTimezone")
        .map(String::as_str)
        == Some("true");
    let zone = if automatic {
        user.timezone.get("automaticTimezone")
    } else {
        user.timezone.get("manualTimezone")
    }?;
    time_in_zone(zone, Utc::now())
}

/// What the clock says in a named zone at `now`. Nothing for a zone the
/// database does not know: a time in the wrong zone is worse than no time.
fn time_in_zone(zone: &str, now: chrono::DateTime<Utc>) -> Option<String> {
    let zone: chrono_tz::Tz = zone.parse().ok()?;
    Some(now.with_timezone(&zone).format("%H:%M").to_string())
}

fn presence_label(presence: Presence) -> &'static str {
    match presence {
        Presence::OutOfOffice => "Out of office",
        other => kit::presence_label(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn a_zone_is_read_by_its_name() {
        let noon = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
        assert_eq!(time_in_zone("UTC", noon).as_deref(), Some("12:00"));
        assert_eq!(time_in_zone("Europe/Moscow", noon).as_deref(), Some("15:00"));
        assert_eq!(time_in_zone("Asia/Kolkata", noon).as_deref(), Some("17:30"));
        // Unknown, or not filled in: say nothing rather than guess.
        assert_eq!(time_in_zone("Mars/Olympus", noon), None);
        assert_eq!(time_in_zone("", noon), None);
    }

    #[test]
    fn the_effective_zone_is_the_one_the_flag_selects() {
        let mut user = User::default();
        user.timezone
            .insert("useAutomaticTimezone".into(), "false".into());
        user.timezone
            .insert("automaticTimezone".into(), "Europe/Moscow".into());
        // Manual is selected and empty: there is no zone to show.
        assert_eq!(local_time(&user), None);

        user.timezone
            .insert("manualTimezone".into(), "Asia/Tokyo".into());
        assert!(local_time(&user).is_some());

        user.timezone
            .insert("useAutomaticTimezone".into(), "true".into());
        assert!(local_time(&user).is_some());
    }
}
