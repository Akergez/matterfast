//! Channel identity, shared by conversation and thread lists.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, AnyElement, App, Div, FontWeight, Hsla};
use mattermost_api::models::Channel;

use super::{kit, Ui};
use crate::state::AppState;

fn initials(name: &str) -> String {
    let letters: String = name
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .take(2)
        .collect();
    if letters.is_empty() {
        "?".into()
    } else {
        letters
    }
}

fn color(name: &str) -> Hsla {
    // FNV-1a is explicit so the same name keeps its colour across launches
    // and platforms. This colour encodes identity, rather than UI status.
    let hash = name.bytes().fold(2_166_136_261_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
    });
    hsla((hash % 360) as f32 / 360., 0.55, 0.34, 1.)
}

pub(super) fn named(name: &str, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(color(name))
        .text_color(hsla(0., 0., 1., 1.))
        .text_size(px((size * 0.34).max(7.)))
        .font_weight(FontWeight::SEMIBOLD)
        .child(initials(name))
}

/// Direct conversations retain their teammate's photo and presence.
pub(super) fn avatar(
    ui: &Rc<Ui>,
    channel: &Channel,
    st: &AppState,
    size: f32,
    cx: &App,
) -> AnyElement {
    let title = st.channel_title(channel);
    match channel.dm_teammate_id(&st.me.id) {
        Some(id) => kit::avatar_with_presence(ui, id, &title, size, st.presence(id), cx),
        None => named(&title, size).into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_use_unicode_words_and_ignore_separators() {
        assert_eq!(initials("Общий канал"), "ОК");
        assert_eq!(initials("  #release-planning_team "), "RP");
        assert_eq!(initials("Development"), "D");
        assert_eq!(initials("中文 频道"), "中频");
        assert_eq!(initials("🔒"), "?");
    }

    #[test]
    fn identity_color_is_stable_and_depends_on_the_whole_name() {
        assert_eq!(color("Development"), color("Development"));
        assert_ne!(color("Development"), color("Design"));
        assert_ne!(color("Общий канал"), color("Общий чат"));
        assert_eq!(color("Development").h, 354. / 360.);
    }
}
