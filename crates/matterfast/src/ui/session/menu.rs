use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::ui::{settings, storage, MenuAction};

impl Ui {
    pub(crate) fn menu_action(self: &Rc<Self>, action: MenuAction, cx: &mut App) {
        match action {
            MenuAction::NewChannel => self.new_channel(cx),
            MenuAction::BrowseChannels => self.browse_channels(cx),
            MenuAction::AccountNotifications => self.account_notifications(cx),
            MenuAction::EditProfile => self.edit_profile(cx),
            MenuAction::QuickSwitch => self.quick_switch(cx),
            MenuAction::SignOut => self.sign_out(cx),
            MenuAction::ScheduledPosts => self.scheduled_posts(cx),
            MenuAction::ChannelMembers => self.channel_members(cx),
            MenuAction::ChannelBookmarks => self.channel_bookmarks(cx),
            MenuAction::BrowseTeams => self.browse_teams(cx),
            MenuAction::NewCategory => self.new_category(cx),
            MenuAction::FocusSearch => self.search_box.focus(cx),
            MenuAction::OpenInbox => self.open_inbox(cx),
            MenuAction::NextUnread => self.step_unread(true, cx),
            MenuAction::PreviousUnread => self.step_unread(false, cx),
            MenuAction::LeaveTeam => self.leave_team(cx),
            MenuAction::EditChannel => self.edit_channel(cx),
            MenuAction::ArchiveChannel => self.archive_channel(cx),
            MenuAction::CustomStatus => self.custom_status(cx),
            MenuAction::ChannelNotifications => self.channel_notifications(cx),
            MenuAction::LeaveChannel => self.leave_channel(cx),
            MenuAction::PinnedPosts => self.show_pinned(cx),
            MenuAction::Storage => storage::show(self, self.avatars.resources(), cx),
            MenuAction::Settings => settings::show(self, cx),
        }
    }
}
