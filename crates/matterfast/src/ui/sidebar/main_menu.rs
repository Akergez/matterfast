use std::rc::Rc;

use gpui_adaptive::{Command, Menu};
use gpui_kit::{AnyElement, IntoElement};

use crate::background::Background;
use crate::ui::kit::{self, Lucide};
use crate::ui::{MenuAction, Ui};

/// The main menu, where the things you do to the *list* live rather than to
/// any one channel.
pub(super) fn main_menu(ui: &Rc<Ui>) -> AnyElement {
    let entries = |entries: &[(&'static str, MenuAction)]| {
        entries
            .iter()
            .map(|&(label, action)| {
                Command::new(label, label).on_trigger(ui.run(move |ui, cx| ui.menu_action(action, cx)))
            })
            .collect::<Vec<_>>()
    };
    Menu::new(kit::icon_button("main-menu", Lucide::Menu, "Main menu"))
        .commands(entries(&[
            ("Jump to…", MenuAction::QuickSwitch),
            ("Scheduled Messages", MenuAction::ScheduledPosts),
            ("New Channel…", MenuAction::NewChannel),
            ("New Category…", MenuAction::NewCategory),
            ("Browse Teams…", MenuAction::BrowseTeams),
            ("Leave This Team", MenuAction::LeaveTeam),
            ("Browse Channels…", MenuAction::BrowseChannels),
        ]))
        .separator()
        .commands(entries(&[("Settings…", MenuAction::Settings), ("Storage…", MenuAction::Storage)]))
        // Leaving a process running with no window is only acceptable if it
        // is visible and stoppable, so it is a checkbox here.
        .command(
            Command::new("background", "Keep Running in Background")
                .checked(Background::enabled())
                .on_trigger(|_, cx| {
                    Background::set_enabled(!Background::enabled());
                    crate::ui::refresh(cx);
                }),
        )
        .command(Command::new("quit", "Quit").on_trigger(|_, cx| cx.quit()))
        .separator()
        .commands(entries(&[
            ("Edit Profile…", MenuAction::EditProfile),
            ("Set a Status…", MenuAction::CustomStatus),
            ("Notifications…", MenuAction::AccountNotifications),
            ("Sign Out", MenuAction::SignOut),
        ]))
        .into_any_element()
}
