use std::rc::Rc;

use gpui_adaptive::prim::{column, row};
use gpui_adaptive::{ActiveColors as _, Command, Menu, Text};
use gpui_kit::prelude::*;
use gpui_kit::{px, AnyElement, App};

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
    let title_for_menu = title.clone();
    drop(st);

    let colors = cx.colors();
    let mut bar = row()
        .flex_none()
        .w_full()
        .h(px(48.))
        .px_2()
        .gap_1()
        .border_b_1()
        .border_color(colors.border);

    if narrow {
        // The channel list is a page behind this one, and this is the way
        // back to it.
        bar = bar.child(
            kit::icon_button("back", Lucide::ChevronLeft, "Channels")
                .on_trigger(ui.run(|ui, cx| ui.pages.show_list(cx)))
                .as_icon_button(),
        );
    }

    bar = bar.child(
        column()
            .flex_1()
            .min_w_0()
            .px_1()
            .child(Text::title(title).truncate())
            .when(!subtitle.is_empty(), |column| {
                column.child(Text::small(subtitle).muted().truncate())
            }),
    );

    if narrow {
        bar = bar.child(
            kit::icon_button("phone-search", Lucide::Search, "Search messages")
                .on_trigger(ui.run(|ui, cx| ui.menu_action(MenuAction::FocusSearch, cx)))
                .as_icon_button(),
        );
    }

    // The inbox button carries its own count, the way a mail client's does:
    // the number is the reason to click it.
    bar = bar.child(
        row()
            .gap_0p5()
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
                .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::OpenInbox, cx)))
                .as_icon_button(),
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
        let asks = agents.into_iter().enumerate().map(|(index, (target, label))| {
            Command::new(("agent", index), label).on_trigger(ui.run(move |ui, cx| {
                match target.split_once(':') {
                    Some(("channel", id)) => ui.dispatch(Action::SelectChannel(id.to_string()), cx),
                    Some(("user", id)) => ui.dispatch(Action::OpenDirectMessage(id.to_string()), cx),
                    _ => {}
                }
            }))
        });
        bar = bar.child(
            Menu::new(kit::icon_button("agents", Lucide::Bot, "Agents"))
                .title("Agents")
                .command(
                    Command::new("catch-me-up", "Catch me up")
                        .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::SummariseUnreads, cx))),
                )
                .separator()
                .commands(asks),
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
    // Red while it would hang up, accent while a call is waiting for you,
    // so the header agrees with the banner instead of looking idle.
    let call = kit::icon_button("call", if in_call { Lucide::PhoneOff } else { Lucide::Phone }, call_tooltip)
        .disabled(!in_call && !chat.calls_available.get())
        .on_trigger(ui.run(|ui, cx| ui.dispatch(Action::ToggleCall, cx)));
    let call = match (in_call, ongoing) {
        (true, _) => call.destructive(),
        (false, true) => call.primary(),
        (false, false) => call,
    };
    bar = bar.child(call.as_icon_button());

    // What you do to *this* channel, as opposed to the message under the
    // pointer or the list in the sidebar.
    let channel_commands = [
        ("details", "Channel Details…", MenuAction::EditChannel),
        ("members", "Members…", MenuAction::ChannelMembers),
        ("bookmarks", "Bookmarks…", MenuAction::ChannelBookmarks),
        ("pinned", "Pinned Messages", MenuAction::PinnedPosts),
        ("notifications", "Notifications…", MenuAction::ChannelNotifications),
        ("leave", "Leave Channel", MenuAction::LeaveChannel),
        ("archive", "Archive Channel", MenuAction::ArchiveChannel),
    ]
    .map(|(id, label, action)| {
        Command::new(id, label).on_trigger(ui.run(move |ui, cx| ui.menu_action(action, cx)))
    });
    bar = bar.child(
        Menu::new(kit::icon_button("channel-menu", Lucide::Ellipsis, "Channel menu"))
            .title(title_for_menu)
            .commands(channel_commands),
    );

    bar.into_any_element()
}
