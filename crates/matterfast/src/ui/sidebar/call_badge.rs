use std::rc::Rc;

use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App};

use crate::state::AppState;
use crate::ui::kit::{self, Lucide};
use crate::ui::Ui;

/// The tooltip on the faces a call draws on its channel's row.
fn call_tooltip(people: usize) -> String {
    match people {
        0 => "A call is starting".to_string(),
        1 => "1 person is in a call".to_string(),
        n => format!("{n} people are in a call"),
    }
}

/// Who is in the call, the way Slack marks a channel: a few faces and the
/// count. Faces beat an icon here — the reason to join is usually who is there.
pub(super) fn call_badge(ui: &Rc<Ui>, people: &[String], st: &AppState, cx: &App) -> AnyElement {
    /// Beyond this the faces are unreadable at 16px and the count carries it.
    const FACES: usize = 3;

    let mut badge = h_flex()
        .gap_0p5()
        .items_center()
        .text_xs()
        .text_color(cx.theme().success)
        .child(Lucide::Headphones);
    for user_id in people.iter().take(FACES) {
        let name = st
            .users
            .get(user_id)
            .map(|u| u.display_name(st.teammate_name_display()))
            .unwrap_or_default();
        badge = badge.child(kit::avatar(ui, user_id, &name, 16.));
    }
    // The count is only news once it exceeds the faces already shown.
    if people.len() > FACES {
        badge = badge.child(format!("+{}", people.len() - FACES));
    }
    kit::with_tooltip("call", badge, call_tooltip(people.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_says_how_many_are_in_it() {
        assert_eq!(call_tooltip(0), "A call is starting");
        assert_eq!(call_tooltip(1), "1 person is in a call");
        assert_eq!(call_tooltip(4), "4 people are in a call");
    }
}
