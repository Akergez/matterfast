use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App, ExternalPaths};

use super::attachments::attachments;
use super::banner::banner;
use super::composer::composer;
use super::feed_list::feed;
use super::header::header;
use crate::ui::chat::status_lines::{call_banner_text, typing_text};
use crate::ui::kit::{self, Lucide};
use crate::ui::{frame_log, Action, Ui};

/// Draws the conversation pane. `narrow` is whether the channel list is a
/// page behind this one rather than a column beside it; `dock` is the call
/// dock, when it belongs under the conversation instead of the sidebar.
pub fn render(ui: &Rc<Ui>, narrow: bool, dock: Option<AnyElement>, cx: &mut App) -> AnyElement {
    let chat = &ui.chat;
    let (has_channel, waiting) = {
        let st = ui.state.borrow();
        let channel = st
            .current_channel
            .as_ref()
            .filter(|id| st.channel(id).is_some());
        // A channel with no posts *yet* and a fetch in flight is loading; one
        // with no posts and nothing in flight is genuinely empty.
        let waiting = channel.is_some_and(|id| {
            chat.loading.get() && st.feeds.get(id).is_none_or(|feed| feed.posts.is_empty())
        });
        (channel.is_some(), waiting)
    };

    let mut pane = v_flex()
        .id("conversation")
        .size_full()
        .min_w_0()
        .bg(cx.theme().background)
        .child(frame_log::timed("header", header(ui, narrow, cx)));

    if !has_channel {
        return pane
            .child(kit::empty_state(
                Lucide::MessageSquarePlus,
                "No channel selected",
                "Pick a channel from the sidebar to start reading.",
                cx,
            ))
            .when_some(dock, |pane, dock| pane.child(dock))
            .into_any_element();
    }

    // Losing the socket is a state of the whole window, not an event, so it
    // gets a banner that stays up rather than a toast that scrolls by.
    if let Some(problem) = chat.connection.borrow().clone() {
        pane = pane.child(banner(problem, cx.theme().warning, cx));
    }

    if let Some(participants) = chat.call_participants.get() {
        let mut call = banner(call_banner_text(participants), cx.theme().success, cx);
        // The banner is where you notice a call, so it is where joining it
        // belongs — the header button is for starting one.
        if !chat.in_call.get() {
            call = call.child(
                Button::new("join-call")
                    .label("Join")
                    .small()
                    .primary()
                    .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleCall, cx))),
            );
        }
        pane = pane.child(call);
    }

    if waiting {
        pane = pane.child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().large()),
        );
    } else {
        pane = pane.child(frame_log::timed("feed", feed(ui, cx)));
    }

    // Editing is a mode, and a mode you cannot see is a trap: the banner says
    // so and offers the way out.
    if chat.editing.borrow().is_some() {
        pane = pane.child(
            banner("Editing a message", cx.theme().primary, cx).child(
                Button::new("cancel-edit")
                    .label("Cancel")
                    .small()
                    .ghost()
                    .on_click(ui.click(|ui, cx| ui.chat.end_edit(cx))),
            ),
        );
    }

    // Sits between the feed and the composer, reserving no space when empty.
    let typing = typing_text(&chat.typing.borrow());
    if !typing.is_empty() {
        pane = pane.child(
            div()
                .flex_none()
                .px_3p5()
                .pb_0p5()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(typing),
        );
    }

    if let Some(row) = attachments(ui, cx) {
        pane = pane.child(row);
    }

    pane.child(frame_log::timed("composer", composer(ui, cx)))
        .when_some(dock, |pane, dock| pane.child(dock))
        // Dropping files anywhere over the conversation attaches them. The
        // target is the whole pane rather than the composer: aiming at a
        // one-line text box is a needlessly precise thing to ask of a drag.
        .on_drop({
            let ui = ui.clone();
            move |paths: &ExternalPaths, _, cx| {
                let paths = paths.paths().to_vec();
                if !paths.is_empty() {
                    ui.dispatch(Action::AttachFiles(paths), cx);
                }
            }
        })
        .into_any_element()
}
