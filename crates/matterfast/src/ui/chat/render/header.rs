use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, FontWeight};

use crate::ui::chat::status_lines::subtitle;
use crate::ui::kit::{self, Lucide};
use crate::ui::{Action, MenuAction, Ui};

/// The header: where you are, and what you can do to it.
pub(super) fn header(ui: &Rc<Ui>, narrow: bool, cx: &mut App) -> AnyElement {
    let chat = &ui.chat;
    let st = ui.state.borrow();
    let channel = st
        .current_channel
        .as_ref()
        .and_then(|id| st.channel(id))
        .cloned();
    let title = channel
        .as_ref()
        .map(|c| st.channel_title(c))
        .unwrap_or_default();
    let subtitle = channel
        .as_ref()
        .map(|c| subtitle(&c.header, chat.member_count.get()))
        .unwrap_or_default();
    let inbox_count = st.inbox_waiting();
    drop(st);

    let theme = cx.theme();
    let mut bar = h_flex()
        .flex_none()
        .w_full()
        .h(px(48.))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(theme.border);

    if narrow {
        // The channel list is a page behind this one, and this is the way
        // back to it.
        bar = bar.child(
            kit::icon_button("back", Lucide::ChevronLeft, "Channels")
                .on_click(ui.click(|ui, cx| ui.split.set_show_content(false, cx))),
        );
    }

    bar = bar.child(
        v_flex()
            .flex_1()
            .min_w_0()
            .px_1()
            .child(
                div()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .when(!subtitle.is_empty(), |column| {
                column.child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(subtitle),
                )
            }),
    );

    // The inbox button carries its own count, the way a mail client's does:
    // the number is the reason to click it.
    bar = bar.child(
        h_flex()
            .gap_0p5()
            .items_center()
            .child(
                kit::icon_button(
                    "inbox",
                    Lucide::Inbox,
                    if inbox_count > 0 {
                        format!("{inbox_count} unread — mentions and threads")
                    } else {
                        "Mentions and threads".to_string()
                    },
                )
                .on_click(ui.click(|ui, cx| ui.dispatch(Action::OpenInbox, cx))),
            )
            .when(inbox_count > 0, |row| {
                row.child(kit::mention_badge(inbox_count, false, cx))
            }),
    );

    // Only there when a server actually has an agent to ask. A menu rather
    // than a button because there are two different things to want from an
    // agent: a summary of here, or a conversation with it.
    let agents = chat.agents.borrow().clone();
    if !agents.is_empty() {
        let ui = ui.clone();
        bar = bar.child(
            kit::icon_button("agents", Lucide::Bot, "Agents").dropdown_menu(
                move |mut menu, _, _| {
                    menu = menu
                        .item(PopupMenuItem::new("Catch me up").on_click(
                            ui.click(|ui, cx| ui.dispatch(Action::SummariseUnreads, cx)),
                        ))
                        .separator();
                    for (target, label) in &agents {
                        let target = target.clone();
                        menu = menu.item(PopupMenuItem::new(label.clone()).on_click(ui.click(
                            move |ui, cx| match target.split_once(':') {
                                Some(("channel", id)) => {
                                    ui.dispatch(Action::SelectChannel(id.to_string()), cx)
                                }
                                Some(("user", id)) => {
                                    ui.dispatch(Action::OpenDirectMessage(id.to_string()), cx)
                                }
                                _ => {}
                            },
                        )));
                    }
                    menu
                },
            ),
        );
    }

    let in_call = chat.in_call.get();
    let ongoing = chat.call_ongoing.get();
    let call_tooltip = chat.calls_reason.borrow().clone().unwrap_or_else(|| {
        match (in_call, ongoing) {
            (true, _) => "Leave the call",
            // A call is running here and we are outside it: the button
            // joins, which is not something an icon alone ever says.
            (false, true) => "Join the call",
            (false, false) => "Start a call",
        }
        .to_string()
    });
    bar = bar.child(
        Button::new("call")
            .icon(if in_call { Lucide::PhoneOff } else { Lucide::Phone })
            .small()
            .tooltip(call_tooltip)
            // Red while it would hang up, accent while a call is waiting for
            // you, so the header agrees with the banner instead of looking
            // idle.
            .when(in_call, |button| button.danger())
            .when(!in_call && ongoing, |button| button.primary())
            .when(!in_call && !ongoing, |button| button.ghost())
            .disabled(!in_call && !chat.calls_available.get())
            .on_click(ui.click(|ui, cx| ui.dispatch(Action::ToggleCall, cx))),
    );

    // What you do to *this* channel, as opposed to the message under the
    // pointer or the list in the sidebar.
    let menu_ui = ui.clone();
    bar = bar.child(
        kit::icon_button("channel-menu", Lucide::Ellipsis, "Channel menu").dropdown_menu(
            move |mut menu, _, _| {
                for (label, action) in [
                    ("Channel Details…", MenuAction::EditChannel),
                    ("Members…", MenuAction::ChannelMembers),
                    ("Bookmarks…", MenuAction::ChannelBookmarks),
                    ("Pinned Messages", MenuAction::PinnedPosts),
                    ("Notifications…", MenuAction::ChannelNotifications),
                    ("Leave Channel", MenuAction::LeaveChannel),
                    ("Archive Channel", MenuAction::ArchiveChannel),
                ] {
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .on_click(menu_ui.click(move |ui, cx| ui.menu_action(action, cx))),
                    );
                }
                menu
            },
        ),
    );

    bar.into_any_element()
}
