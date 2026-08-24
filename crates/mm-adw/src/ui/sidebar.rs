//! Pane 1 (team rail) and pane 2 (channel list).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use mattermost_api::models::{Channel, ChannelType, Presence, Team, UnreadState};

use crate::avatars::Avatars;
use crate::state::SharedState;

/// The narrow rail of teams, pane one.
pub struct TeamRail {
    pub widget: gtk::Box,
    /// Only shown when the window is collapsed and the rail becomes a
    /// full-width page that needs somewhere to put a back button.
    pub header: adw::HeaderBar,
    list: gtk::ListBox,
    /// Guards against re-entering the selection handler while we rebuild.
    updating: Rc<RefCell<bool>>,
}

impl TeamRail {
    pub fn new(on_select: impl Fn(String) + 'static) -> Self {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        list.add_css_class("navigation-sidebar");

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();

        let header = adw::HeaderBar::builder()
            .title_widget(
                &gtk::Label::builder()
                    .label("Teams")
                    .css_classes(["heading"])
                    .build(),
            )
            .show_end_title_buttons(false)
            .visible(false)
            .build();

        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        widget.add_css_class("team-rail");
        widget.append(&header);
        widget.append(&scroller);

        let updating = Rc::new(RefCell::new(false));
        list.connect_row_selected({
            let updating = updating.clone();
            move |_, row| {
                if *updating.borrow() {
                    return;
                }
                if let Some(row) = row {
                    if let Some(id) = unsafe { row.data::<String>("team-id") } {
                        on_select(unsafe { id.as_ref() }.clone());
                    }
                }
            }
        });

        TeamRail {
            widget,
            header,
            list,
            updating,
        }
    }

    pub fn refresh(&self, teams: &[Team], current: Option<&str>) {
        *self.updating.borrow_mut() = true;
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        for team in teams.iter().filter(|t| t.delete_at == 0) {
            let avatar = adw::Avatar::builder()
                .size(40)
                .text(&team.display_name)
                .show_initials(true)
                .build();
            avatar.add_css_class("team-avatar");

            let row = gtk::ListBoxRow::builder()
                .child(&avatar)
                .tooltip_text(&team.display_name)
                .build();
            unsafe { row.set_data("team-id", team.id.clone()) };
            self.list.append(&row);

            if Some(team.id.as_str()) == current {
                self.list.select_row(Some(&row));
            }
        }
        *self.updating.borrow_mut() = false;
    }
}

/// The channel list, pane two.
pub struct ChannelSidebar {
    pub widget: gtk::Box,
    list: gtk::ListBox,
    title: gtk::Label,
    updating: Rc<RefCell<bool>>,
}

impl ChannelSidebar {
    pub fn new(on_select: impl Fn(String) + 'static) -> Self {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        list.add_css_class("navigation-sidebar");

        // Category headers are non-selectable rows, so skip them when the user
        // arrows through the list.
        list.set_header_func(|_row, _before| {});

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();

        let title = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        title.add_css_class("heading");

        let header = adw::HeaderBar::builder()
            .title_widget(&title)
            .show_end_title_buttons(false)
            .build();

        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        widget.append(&header);
        widget.append(&scroller);

        let updating = Rc::new(RefCell::new(false));
        list.connect_row_selected({
            let updating = updating.clone();
            move |_, row| {
                if *updating.borrow() {
                    return;
                }
                if let Some(row) = row {
                    if let Some(id) = unsafe { row.data::<String>("channel-id") } {
                        on_select(unsafe { id.as_ref() }.clone());
                    }
                }
            }
        });

        ChannelSidebar {
            widget,
            list,
            title,
            updating,
        }
    }

    pub fn refresh(&self, state: &SharedState, avatars: &Avatars) {
        *self.updating.borrow_mut() = true;
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        let st = state.borrow();
        let team_name = st
            .current_team
            .as_ref()
            .and_then(|id| st.teams.iter().find(|t| &t.id == id))
            .map(|t| t.display_name.clone())
            .unwrap_or_else(|| "Mattermost".to_string());
        self.title.set_text(&team_name);

        for (category, channels) in st.sidebar_groups() {
            self.list.append(&category_header(&category.display_name));

            for channel in channels {
                let unread = st.unread(&channel.id);
                let in_call = st.active_calls.contains_key(&channel.id);
                let title = st.channel_title(&channel);
                // A DM is a person, so it gets that person's face with the
                // presence badge, exactly like a message row.
                let icon = channel
                    .dm_teammate_id(&st.me.id)
                    .map(|user_id| dm_avatar(avatars, user_id, &title, st.presence(user_id)));
                let row = channel_row(&channel, &title, unread, in_call, icon);
                unsafe { row.set_data("channel-id", channel.id.clone()) };
                self.list.append(&row);

                if st.current_channel.as_deref() == Some(channel.id.as_str()) {
                    self.list.select_row(Some(&row));
                }
            }
        }
        drop(st);
        *self.updating.borrow_mut() = false;
    }
}

fn category_header(name: &str) -> gtk::ListBoxRow {
    let label = gtk::Label::builder()
        .label(name.to_uppercase())
        .xalign(0.0)
        .margin_top(12)
        .margin_bottom(2)
        .margin_start(6)
        .build();
    label.add_css_class("dim-label");
    label.add_css_class("caption-heading");

    let row = gtk::ListBoxRow::builder()
        .child(&label)
        .selectable(false)
        .activatable(false)
        .build();
    row.add_css_class("background");
    row
}

fn dm_avatar(
    avatars: &Avatars,
    user_id: &str,
    display_name: &str,
    presence: Presence,
) -> gtk::Widget {
    let avatar = adw::Avatar::builder().size(20).build();
    avatars.apply(&avatar, user_id, display_name);
    super::profile::with_presence(&avatar, presence).upcast()
}

fn channel_row(
    channel: &Channel,
    title: &str,
    unread: UnreadState,
    in_call: bool,
    icon: Option<gtk::Widget>,
) -> gtk::ListBoxRow {
    // Public channels get a literal "#", the way Mattermost writes them; the
    // rest get an icon. A glyph also sidesteps icon-theme gaps on minimal
    // systems, where a missing SVG loader turns every symbolic icon into a
    // broken-image box.
    let icon: gtk::Widget = match icon {
        Some(widget) => widget,
        None => match channel.r#type {
        ChannelType::Open => {
            let hash = gtk::Label::new(Some("#"));
            hash.add_css_class("dim-label");
            hash.set_width_request(16);
            hash.upcast()
        }
        _ => {
            let image = gtk::Image::from_icon_name(channel_icon(&channel.r#type));
            image.add_css_class("dim-label");
            image.upcast()
        }
        },
    };

    let label = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();

    let row_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(4)
        .margin_bottom(4)
        .build();
    row_box.append(&icon);
    row_box.append(&label);

    // A call in this channel matters more than an unread badge, so it goes
    // first and is always shown.
    if in_call {
        let call = gtk::Image::from_icon_name("call-start-symbolic");
        call.add_css_class("success");
        call.set_tooltip_text(Some("Call in progress"));
        row_box.append(&call);
    }

    if unread.mentions > 0 {
        let badge = gtk::Label::new(Some(&unread.mentions.to_string()));
        badge.add_css_class("mention-badge");
        if unread.urgent {
            badge.add_css_class("urgent");
        }
        row_box.append(&badge);
    }

    // Muted channels still show mentions, but not bold-for-messages: that is
    // exactly the rule the official clients use.
    if unread.is_unread() {
        label.add_css_class("channel-row-unread");
    }
    if unread.muted {
        row_box.add_css_class("channel-row-muted");
    }

    gtk::ListBoxRow::builder().child(&row_box).build()
}

fn channel_icon(kind: &ChannelType) -> &'static str {
    match kind {
        // Open is handled with a "#" glyph by the caller.
        ChannelType::Open => "network-workgroup-symbolic",
        ChannelType::Private => "changes-prevent-symbolic",
        ChannelType::Direct => "avatar-default-symbolic",
        ChannelType::Group => "system-users-symbolic",
        _ => "user-available-symbolic",
    }
}
