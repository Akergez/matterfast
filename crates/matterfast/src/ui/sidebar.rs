//! The channel sidebar: an account/team switcher in the header, the list below.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, ElementId, FontWeight, Window};
use mattermost_api::models::{CategoryType, Channel, ChannelType, SidebarCategory};

use super::kit::{self, Lucide};
use super::message::{emoji_element, status_is_live};
use super::{Action, MenuAction, Ui};
use crate::background::Background;
use crate::state::AppState;

/// What the channel row's own menu can ask for.
#[derive(Debug, Clone)]
pub enum RowAction {
    MarkRead,
    MarkUnread,
    SetMuted(bool),
    MoveTo(String),
    RenameCategory,
    DeleteCategory,
}

/// The channel list, pane one.
pub struct ChannelSidebar;

impl ChannelSidebar {
    /// The list is drawn from the state every frame, so all a refresh has to
    /// do is ask for a frame.
    pub fn refresh(&self, cx: &mut App) {
        cx.refresh_windows();
    }
}

/// What a channel row's menu offers, worked out from the state rather than
/// kept: whether it is read, whether it is muted, and where else it could go.
fn row_menu(channel_id: &str, st: &AppState) -> Vec<(String, RowAction)> {
    let muted = st
        .memberships
        .get(channel_id)
        .is_some_and(|m| m.is_muted());
    let mut entries = Vec::new();
    if st.unread(channel_id).is_unread() {
        entries.push(("Mark as read".to_string(), RowAction::MarkRead));
    } else {
        entries.push(("Mark as unread".to_string(), RowAction::MarkUnread));
    }
    entries.push((
        if muted { "Unmute" } else { "Mute" }.to_string(),
        RowAction::SetMuted(!muted),
    ));

    // Where it is now is not somewhere to move it to.
    let current = st
        .categories
        .categories
        .iter()
        .find(|c| c.channel_ids.iter().any(|id| id == channel_id))
        .map(|c| c.id.clone())
        .unwrap_or_default();
    for category in &st.categories.categories {
        if category.id == current {
            continue;
        }
        entries.push((
            format!("Move to {}", category.display_name),
            RowAction::MoveTo(category.id.clone()),
        ));
    }
    entries
}

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
fn call_badge(ui: &Rc<Ui>, people: &[String], st: &AppState, cx: &App) -> AnyElement {
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

fn category_header(ui: &Rc<Ui>, category: &SidebarCategory, cx: &App) -> AnyElement {
    let header = div()
        .id(ElementId::Name(format!("category-{}", category.id).into()))
        .w_full()
        .px_3()
        .pt_3()
        .pb_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(category.display_name.clone());

    // Custom categories can be renamed and deleted; the built-in ones
    // (Favourites, Channels, Direct Messages) cannot, and offering it would
    // only produce a server error.
    if category.r#type != CategoryType::Custom {
        return header.into_any_element();
    }
    let ui = ui.clone();
    let id = category.id.clone();
    header
        .context_menu(move |menu, _, _| {
            let rename = id.clone();
            let delete = id.clone();
            menu.item(PopupMenuItem::new("Rename…").on_click(ui.click(move |ui, cx| {
                ui.dispatch(Action::Row(rename.clone(), RowAction::RenameCategory), cx)
            })))
            .item(PopupMenuItem::new("Delete").on_click(ui.click(move |ui, cx| {
                ui.dispatch(Action::Row(delete.clone(), RowAction::DeleteCategory), cx)
            })))
        })
        .into_any_element()
}

fn channel_row(ui: &Rc<Ui>, channel: &Channel, st: &AppState, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let title = st.channel_title(channel);
    let unread = st.unread(&channel.id);
    let selected = st.current_channel.as_deref() == Some(channel.id.as_str());
    let teammate = channel.dm_teammate_id(&st.me.id);

    // Public channels get a literal "#", the way Mattermost writes them; a
    // DM is a person, so it gets that person's face with the presence dot,
    // exactly like a message row.
    let icon: AnyElement = match (teammate, &channel.r#type) {
        (Some(user_id), _) => {
            kit::avatar_with_presence(ui, user_id, &title, 20., st.presence(user_id), cx)
        }
        (None, ChannelType::Open) => div()
            .w(px(20.))
            .flex_none()
            .text_center()
            .text_color(theme.muted_foreground)
            .child("#")
            .into_any_element(),
        (None, kind) => div()
            .w(px(20.))
            .flex_none()
            .flex()
            .justify_center()
            .text_color(theme.muted_foreground)
            .child(match kind {
                ChannelType::Private => Lucide::Lock,
                ChannelType::Group => Lucide::Users,
                _ => Lucide::User,
            })
            .into_any_element(),
    };

    let mut row = h_flex()
        .id(ElementId::Name(format!("channel-{}", channel.id).into()))
        .w_full()
        .h(px(32.))
        .px_2()
        .gap_2()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.sidebar_accent))
        .when(!selected, |row| row.hover(|style| style.bg(theme.list_hover)))
        // Muted channels still show mentions, but not bold-for-messages: that
        // is exactly the rule the official clients use.
        .when(unread.muted, |row| row.opacity(0.6))
        .child(icon)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(unread.is_unread(), |label| {
                    label.font_weight(FontWeight::SEMIBOLD)
                })
                .child(title),
        );

    // A DM is a person, and their status says whether writing to them is
    // worth doing now.
    if let Some(status) = teammate
        .and_then(|id| st.users.get(id))
        .and_then(|user| user.custom_status())
        .filter(|status| !status.emoji.is_empty() && status_is_live(status))
    {
        row = row.child(kit::with_tooltip(
            "status",
            emoji_element(ui, &status.emoji, 16.),
            super::message::custom_status_tooltip(&status),
        ));
    }

    // A call in this channel matters more than an unread badge, so it goes
    // first and is always shown.
    if let Some(people) = st.active_calls.get(&channel.id) {
        row = row.child(call_badge(ui, people, st, cx));
    }

    // Something unsent here. Shown even on a muted channel: it is your own
    // text waiting, not someone else's noise.
    if st.drafts.contains_key(&channel.id) {
        row = row.child(kit::with_tooltip(
            "draft",
            div()
                .text_color(theme.muted_foreground)
                .child(Lucide::Pencil),
            "You have an unsent message here",
        ));
    }

    if unread.mentions > 0 {
        row = row.child(kit::mention_badge(unread.mentions, unread.urgent, cx));
    } else if unread.is_unread() && !unread.muted {
        row = row.child(kit::unread_dot(cx));
    }

    let entries = row_menu(&channel.id, st);
    let menu_ui = ui.clone();
    let channel_id = channel.id.clone();
    let select = channel.id.clone();
    row.on_click(ui.click(move |ui, cx| {
        ui.dispatch(Action::SelectChannel(select.clone()), cx)
    }))
    // Mute it, mark it, or file it under a different category: all things
    // people expect to reach from the row itself rather than from a settings
    // screen.
    .context_menu(move |mut menu, _, _| {
        for (label, action) in &entries {
            let action = action.clone();
            let channel_id = channel_id.clone();
            menu = menu.item(PopupMenuItem::new(label.clone()).on_click(menu_ui.click(
                move |ui, cx| ui.dispatch(Action::Row(channel_id.clone(), action.clone()), cx),
            )));
        }
        menu
    })
    .into_any_element()
}

/// The round avatar in the sidebar header and what drops out of it: who you
/// are signed in as, and the teams to switch between. A rail of teams would
/// cost a permanent column to say what a popover says on demand.
fn switcher(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let (me_id, name, presence) = {
        let st = ui.state.borrow();
        (
            st.me.id.clone(),
            st.me.display_name(st.teammate_name_display()),
            st.presence(&st.me.id),
        )
    };
    let content_ui = ui.clone();
    Popover::new("account")
        .trigger(
            Button::new("account-button")
                .ghost()
                .small()
                .tooltip("Account and teams")
                .child(kit::avatar_with_presence(
                    ui, &me_id, &name, 24., presence, cx,
                )),
        )
        .content(move |_, _, cx| {
            let ui = &content_ui;
            let popover = cx.entity();
            // Close the popover a button lives in, so the choice registers
            // as made.
            let close = move |window: &mut Window, cx: &mut App| {
                popover.update(cx, |state, cx| state.dismiss(window, cx));
            };
            let st = ui.state.borrow();
            let theme = cx.theme();
            let me = &st.me;
            let display = me.display_name(st.teammate_name_display());

            let account = h_flex()
                .gap_3()
                .items_center()
                .child(kit::avatar_with_presence(
                    ui,
                    &me.id,
                    &display,
                    40.,
                    st.presence(&me.id),
                    cx,
                ))
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(display.clone()),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("@{}", me.username)),
                        ),
                );

            // Setting your own status belongs with your own name, which is
            // here.
            let mut statuses = h_flex().gap_1();
            for (index, (label, value)) in [
                ("Online", "online"),
                ("Away", "away"),
                ("Do not disturb", "dnd"),
                ("Offline", "offline"),
            ]
            .into_iter()
            .enumerate()
            {
                let close = close.clone();
                let ui = ui.clone();
                statuses = statuses.child(
                    Button::new(("status", index))
                        .ghost()
                        .small()
                        .flex_1()
                        .tooltip(label)
                        .child(
                            div().size(px(10.)).rounded_full().bg(kit::presence_color(
                                mattermost_api::models::Presence::from(value),
                                cx,
                            )),
                        )
                        .on_click(move |_, window, cx| {
                            close(window, cx);
                            ui.dispatch(Action::SetStatus(value.to_string()), cx);
                        }),
                );
            }

            let mut teams = v_flex().gap_0p5();
            for team in st.teams.iter().filter(|t| t.delete_at == 0) {
                let current = st.current_team.as_deref() == Some(team.id.as_str());
                let close = close.clone();
                let ui = ui.clone();
                let team_id = team.id.clone();
                let mut row = h_flex()
                    .id(ElementId::Name(format!("team-{}", team.id).into()))
                    .h(px(34.))
                    .px_2()
                    .gap_3()
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .when(current, |row| row.bg(theme.accent))
                    .hover(|style| style.bg(theme.list_hover))
                    .child(
                        gpui_kit::component::avatar::Avatar::new()
                            .name(team.display_name.clone())
                            .with_size(gpui_kit::component::Size::Size(px(24.))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(team.display_name.clone()),
                    );
                // Where something is waiting, so switching teams is a
                // decision rather than a guess.
                if let Some((messages, mentions)) = st.team_unreads.get(&team.id) {
                    if *mentions > 0 {
                        row = row.child(kit::mention_badge(*mentions, false, cx));
                    } else if *messages > 0 {
                        row = row.child(kit::unread_dot(cx));
                    }
                }
                teams = teams.child(row.on_click(move |_, window, cx| {
                    close(window, cx);
                    ui.dispatch(Action::SelectTeam(team_id.clone()), cx);
                }));
            }

            v_flex()
                .w(px(260.))
                .gap_2()
                .child(account)
                .child(statuses)
                .child(div().h(px(1.)).bg(theme.border))
                .child(
                    div()
                        .px_1()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.muted_foreground)
                        .child("TEAMS"),
                )
                .child(
                    div()
                        .id("teams")
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .child(teams),
                )
        })
        .into_any_element()
}

/// The main menu, where the things you do to the *list* live rather than to
/// any one channel.
fn main_menu(ui: &Rc<Ui>) -> AnyElement {
    let ui = ui.clone();
    kit::icon_button("main-menu", Lucide::Menu, "Main menu")
        .dropdown_menu(move |mut menu, _, _| {
            let entry = |label: &'static str, action: MenuAction| {
                PopupMenuItem::new(label).on_click(ui.click(move |ui, cx| ui.menu_action(action, cx)))
            };
            for (label, action) in [
                ("Jump to…", MenuAction::QuickSwitch),
                ("Scheduled Messages", MenuAction::ScheduledPosts),
                ("New Channel…", MenuAction::NewChannel),
                ("New Category…", MenuAction::NewCategory),
                ("Browse Teams…", MenuAction::BrowseTeams),
                ("Leave This Team", MenuAction::LeaveTeam),
                ("Browse Channels…", MenuAction::BrowseChannels),
            ] {
                menu = menu.item(entry(label, action));
            }
            menu = menu
                .separator()
                .item(entry("Settings…", MenuAction::Settings))
                .item(entry("Storage…", MenuAction::Storage))
                // Leaving a process running with no window is only acceptable
                // if it is visible and stoppable, so it is a checkbox here.
                .item(
                    PopupMenuItem::new("Keep Running in Background")
                        .checked(Background::enabled())
                        .on_click(|_, _, cx| {
                            Background::set_enabled(!Background::enabled());
                            cx.refresh_windows();
                        }),
                )
                .item(PopupMenuItem::new("Quit").on_click(|_, _, cx| cx.quit()))
                .separator();
            for (label, action) in [
                ("Edit Profile…", MenuAction::EditProfile),
                ("Set a Status…", MenuAction::CustomStatus),
                ("Notifications…", MenuAction::AccountNotifications),
                ("Sign Out", MenuAction::SignOut),
            ] {
                menu = menu.item(entry(label, action));
            }
            menu
        })
        .into_any_element()
}

/// Draws the sidebar. `dock` is the call dock, which is pinned under the
/// channel list for as long as a call runs.
pub fn render(ui: &Rc<Ui>, dock: Option<AnyElement>, cx: &mut App) -> AnyElement {
    let st = ui.state.borrow();
    let theme = cx.theme();
    let team_name = st
        .current_team
        .as_ref()
        .and_then(|id| st.teams.iter().find(|t| &t.id == id))
        .map(|t| t.display_name.clone())
        .unwrap_or_else(|| "Matterfast".to_string());

    let header = h_flex()
        .flex_none()
        .h(px(48.))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(theme.sidebar_border)
        .child(switcher(ui, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(team_name),
        )
        .child(
            // Going to a channel by name: the list's own search, as opposed
            // to the search of messages in the title bar.
            kit::icon_button("find-channel", Lucide::ListFilter, "Find channel  (Ctrl+K)")
                .on_click(ui.click(|ui, cx| ui.menu_action(MenuAction::QuickSwitch, cx))),
        )
        .child(main_menu(ui));

    let mut list = v_flex().id("channels").flex_1().min_h_0().px_1p5().pb_2();
    for (category, channels) in st.sidebar_groups() {
        list = list.child(category_header(ui, &category, cx));
        for channel in &channels {
            list = list.child(channel_row(ui, channel, &st, cx));
        }
    }
    drop(st);

    let pane = v_flex()
        .size_full()
        .bg(theme.sidebar)
        .text_color(theme.sidebar_foreground)
        .border_r_1()
        .border_color(theme.sidebar_border)
        .child(header);

    pane.child(list.overflow_y_scroll())
        .when_some(dock, |pane, dock| pane.child(dock))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::models::{ChannelMember, ClientConfig, User};
    use mattermost_api::Client;

    fn state() -> AppState {
        let client = Client::new("http://x.test").unwrap();
        let mut st = AppState::new(client, User::default(), ClientConfig::default(), false);
        st.channels.insert(
            "c1".into(),
            Channel {
                id: "c1".into(),
                total_msg_count: 10,
                ..Default::default()
            },
        );
        st.memberships.insert(
            "c1".into(),
            ChannelMember {
                channel_id: "c1".into(),
                msg_count: 10,
                ..Default::default()
            },
        );
        for (id, name, channels) in [
            ("fav", "Favorites", vec![]),
            ("chan", "Channels", vec!["c1".to_string()]),
        ] {
            st.categories.categories.push(SidebarCategory {
                id: id.into(),
                display_name: name.into(),
                channel_ids: channels,
                ..Default::default()
            });
        }
        st
    }

    fn labels(entries: &[(String, RowAction)]) -> Vec<&str> {
        entries.iter().map(|(label, _)| label.as_str()).collect()
    }

    #[test]
    fn the_menu_offers_the_opposite_of_what_is() {
        let mut st = state();
        assert_eq!(
            labels(&row_menu("c1", &st)),
            ["Mark as unread", "Mute", "Move to Favorites"]
        );

        // Something arrived.
        st.channels.get_mut("c1").unwrap().total_msg_count = 11;
        assert_eq!(
            labels(&row_menu("c1", &st)),
            ["Mark as read", "Mute", "Move to Favorites"]
        );

        // Muted, the same message no longer counts as unread — only a
        // mention would — so there is nothing left to mark read.
        st.memberships
            .get_mut("c1")
            .unwrap()
            .notify_props
            .insert("mark_unread".into(), "mention".into());
        assert_eq!(
            labels(&row_menu("c1", &st)),
            ["Mark as unread", "Unmute", "Move to Favorites"]
        );
    }

    #[test]
    fn a_channel_is_never_offered_a_move_to_where_it_already_is() {
        let st = state();
        assert!(!labels(&row_menu("c1", &st)).contains(&"Move to Channels"));
    }

    #[test]
    fn a_call_says_how_many_are_in_it() {
        assert_eq!(call_tooltip(0), "A call is starting");
        assert_eq!(call_tooltip(1), "1 person is in a call");
        assert_eq!(call_tooltip(4), "4 people are in a call");
    }
}
