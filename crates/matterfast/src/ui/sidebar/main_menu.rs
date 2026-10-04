use std::rc::Rc;

use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::{AnyElement, IntoElement};

use crate::background::Background;
use crate::ui::kit::{self, Lucide};
use crate::ui::{MenuAction, Ui};

/// The main menu, where the things you do to the *list* live rather than to
/// any one channel.
pub(super) fn main_menu(ui: &Rc<Ui>) -> AnyElement {
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
