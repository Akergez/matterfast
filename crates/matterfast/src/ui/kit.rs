//! Small pieces every pane draws with: a face with its presence dot, a
//! mention badge, an empty-state page.
//!
//! Nothing here holds state or knows what a channel is. These exist so that
//! the same thing looks the same in the sidebar, the feed and the inbox
//! without three copies of how.

use std::rc::Rc;

use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, Sizable, Size};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, Div, ElementId, Hsla, SharedString, Window};
use mattermost_api::models::Presence;

use super::Ui;

/// The full icon catalogue. The component library's own enum only carries the
/// hundred icons its own widgets use.
pub use gpui_kit::assets::IconName as Lucide;

/// What colour a presence is drawn in.
pub fn presence_color(presence: Presence, cx: &App) -> Hsla {
    match presence {
        Presence::Online => cx.theme().success,
        Presence::Away => cx.theme().warning,
        Presence::Dnd => cx.theme().danger,
        _ => cx.theme().muted_foreground.opacity(0.6),
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

/// Somebody's face at a given size: the picture when it has arrived, their
/// initials until then. Asking for the picture is what starts it downloading.
pub fn avatar(ui: &Rc<Ui>, user_id: &str, name: &str, size: f32) -> Div {
    let avatar = Avatar::new()
        .name(SharedString::from(name.to_string()))
        .with_size(Size::Size(px(size)));
    // The component scales the box the initials sit in with the avatar but
    // leaves the letters at the surrounding text size, so at 16px they spill
    // out of the circle. The letters are scaled here instead.
    div()
        .flex_none()
        .text_size(px((size * 0.34).max(7.0)))
        .child(match ui.avatars.texture(user_id) {
            Some(picture) => avatar.src(picture),
            None => avatar,
        })
}

/// A face with the status dot on its corner.
pub fn avatar_with_presence(
    ui: &Rc<Ui>,
    user_id: &str,
    name: &str,
    size: f32,
    presence: Presence,
    cx: &App,
) -> AnyElement {
    let dot = (size / 3.4).max(7.0);
    div()
        .relative()
        .flex_none()
        .child(avatar(ui, user_id, name, size))
        .child(
            div()
                .absolute()
                .right(px(-1.))
                .bottom(px(-1.))
                .size(px(dot))
                .rounded_full()
                .border_2()
                .border_color(cx.theme().background)
                .bg(presence_color(presence, cx)),
        )
        .into_any_element()
}

/// The count of things that named you: a pill, red when it is urgent.
pub fn mention_badge(count: i64, urgent: bool, cx: &App) -> AnyElement {
    div()
        .flex_none()
        .px_1p5()
        .min_w(px(18.))
        .h(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .text_xs()
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .bg(if urgent {
            cx.theme().danger
        } else {
            cx.theme().primary
        })
        .text_color(if urgent {
            cx.theme().danger_foreground
        } else {
            cx.theme().primary_foreground
        })
        .child(count.to_string())
        .into_any_element()
}

/// Unread without a mention is a dot: enough to notice, not enough to demand
/// a number be read.
pub fn unread_dot(cx: &App) -> AnyElement {
    div()
        .flex_none()
        .size(px(8.))
        .rounded_full()
        .bg(cx.theme().primary)
        .into_any_element()
}

/// A small tag such as URGENT, in the colour of what it warns about.
pub fn tag(text: &'static str, color: Hsla, foreground: Hsla) -> AnyElement {
    div()
        .flex_none()
        .px_1p5()
        .rounded_sm()
        .text_xs()
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .bg(color)
        .text_color(foreground)
        .child(text)
        .into_any_element()
}

/// A flat icon button with a tooltip — the shape nearly every toolbar button
/// in the window has.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: impl Into<Icon>,
    tooltip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .icon(icon.into())
        .ghost()
        .small()
        .tooltip(tooltip)
}

/// A page that says why there is nothing here instead of leaving a blank gap.
pub fn empty_state(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    cx: &App,
) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .p_6()
        .child(
            icon.into()
                .with_size(Size::Size(px(40.)))
                .text_color(cx.theme().muted_foreground.opacity(0.6)),
        )
        .child(
            div()
                .text_lg()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(
            div()
                .text_sm()
                .text_center()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
        )
        .into_any_element()
}

/// A rule with a label in the middle of it: the day, or "New messages".
pub fn labelled_rule(label: impl Into<SharedString>, color: Hsla, cx: &App) -> AnyElement {
    let line = || div().flex_1().h(px(1.)).bg(color.opacity(0.35));
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .py_1()
        .child(line())
        .child(
            div()
                .flex_none()
                .text_xs()
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(if color == cx.theme().border {
                    cx.theme().muted_foreground
                } else {
                    color
                })
                .child(label.into()),
        )
        .child(line())
        .into_any_element()
}

/// Text that explains itself on hover.
pub fn with_tooltip(
    id: impl Into<ElementId>,
    child: impl IntoElement,
    tooltip: impl Into<SharedString>,
) -> AnyElement {
    let tooltip: SharedString = tooltip.into();
    div()
        .id(id)
        .flex_none()
        .child(child)
        .tooltip(move |window: &mut Window, cx: &mut App| {
            Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .into_any_element()
}
