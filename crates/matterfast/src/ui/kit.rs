//! Small pieces every pane draws with: a face with its presence dot, a
//! mention badge, an empty-state page.
//!
//! Nothing here holds state or knows what a channel is. These exist so that
//! the same thing looks the same in the sidebar, the feed and the inbox
//! without three copies of how. They are the design system's (gpui-adaptive)
//! parts with what Mattermost calls things.

use std::rc::Rc;

use gpui_adaptive::prim::{row, tooltip};
use gpui_adaptive::{ActiveColors as _, Command, Count, Empty, Icon, Text, Tone};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, rems, AnyElement, App, Div, ElementId, FontWeight, Hsla, SharedString};
use mattermost_api::models::Presence;

use super::Ui;

/// The full icon catalogue. The component library's own enum only carries the
/// hundred icons its own widgets use.
pub use gpui_adaptive::Lucide;

/// What colour a presence is drawn in.
pub fn presence_color(presence: Presence, cx: &App) -> Hsla {
    let colors = cx.colors();
    match presence {
        Presence::Online => colors.success,
        Presence::Away => colors.warning,
        Presence::Dnd => colors.danger,
        _ => colors.muted.opacity(0.6),
    }
}

/// What a presence is called.
pub fn presence_label(presence: Presence) -> &'static str {
    match presence {
        Presence::Online => "Online",
        Presence::Away => "Away",
        Presence::Dnd => "Do not disturb",
        _ => "Offline",
    }
}

/// The design system's presence for Mattermost's.
fn presence_of(presence: Presence) -> gpui_adaptive::Presence {
    match presence {
        Presence::Online => gpui_adaptive::Presence::Online,
        Presence::Away => gpui_adaptive::Presence::Away,
        Presence::Dnd => gpui_adaptive::Presence::Busy,
        _ => gpui_adaptive::Presence::Offline,
    }
}

/// The letters a face shows until its picture arrives: the first of the
/// first two words.
pub fn initials(name: &str) -> SharedString {
    let letters: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .collect();
    letters.into()
}

/// The face of `user_id`, at `size` pixels: the picture when it has
/// arrived, their initials until then. Asking for the picture is what
/// starts it downloading.
fn face(ui: &Rc<Ui>, user_id: &str, name: &str, size: f32) -> gpui_adaptive::Avatar {
    let face = gpui_adaptive::Avatar::new(SharedString::from(format!("avatar-{user_id}")), initials(name))
        .size(rems(size / 16.));
    match ui.avatars.texture(user_id) {
        Some(picture) => face.picture(picture),
        None => face,
    }
}

/// Somebody's face at a given size.
pub fn avatar(ui: &Rc<Ui>, user_id: &str, name: &str, size: f32) -> Div {
    div().flex_none().child(face(ui, user_id, name, size))
}

/// A face with the status dot on its corner.
pub fn avatar_with_presence(
    ui: &Rc<Ui>,
    user_id: &str,
    name: &str,
    size: f32,
    presence: Presence,
    _: &App,
) -> AnyElement {
    face(ui, user_id, name, size)
        .presence(presence_of(presence))
        .into_any_element()
}

/// The count of things that named you: a pill, red when it is urgent.
pub fn mention_badge(count: i64, urgent: bool, _: &App) -> AnyElement {
    div()
        .flex_none()
        .child(Count::new(count.max(0) as usize).urgent(urgent))
        .into_any_element()
}

/// Unread without a mention is a dot: enough to notice, not enough to demand
/// a number be read.
pub fn unread_dot(cx: &App) -> AnyElement {
    div()
        .flex_none()
        .size(px(8.))
        .rounded_full()
        .bg(cx.colors().accent)
        .into_any_element()
}

/// A small tag such as URGENT, in the colour of what it warns about.
pub fn tag(text: &'static str, tone: Tone, cx: &App) -> AnyElement {
    let color = cx.colors().tone(tone);
    div()
        .flex_none()
        .px_1p5()
        .rounded_sm()
        .bg(color.opacity(0.15))
        .child(Text::small(text).tone(tone).strong())
        .into_any_element()
}

/// A command shown as an icon with its label in the tooltip: the shape
/// nearly every toolbar button in the window has. Give it its work with
/// `on_trigger` and place it with `as_icon_button`, or put it behind a
/// [`gpui_adaptive::Menu`] or a [`gpui_adaptive::Popup`].
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: impl Into<Icon>,
    label: impl Into<SharedString>,
) -> Command {
    Command::new(id, label).icon(icon)
}

/// A page that says why there is nothing here instead of leaving a blank gap.
pub fn empty_state(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    _: &App,
) -> AnyElement {
    Empty::new(icon, title).body(description).into_any_element()
}

/// A rule with a label in the middle of it: the day, or "New messages",
/// in the colour of `tone` or quiet without one.
pub fn labelled_rule(label: impl Into<SharedString>, tone: Option<Tone>, cx: &App) -> AnyElement {
    let colors = cx.colors();
    let color = tone.map_or(colors.border, |tone| colors.tone(tone));
    let line = || div().flex_1().h(px(1.)).bg(color.opacity(0.35));
    let label = div()
        .flex_none()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(tone.map_or(colors.muted, |tone| colors.tone(tone)))
        .child(label.into());
    row()
        .w_full()
        .gap_2()
        .py_1()
        .child(line())
        .child(label)
        .child(line())
        .into_any_element()
}

/// Text that explains itself on hover.
pub fn with_tooltip(
    id: impl Into<ElementId>,
    child: impl IntoElement,
    text: impl Into<SharedString>,
) -> AnyElement {
    div()
        .id(id)
        .flex_none()
        .child(child)
        .tooltip(tooltip(text))
        .into_any_element()
}
