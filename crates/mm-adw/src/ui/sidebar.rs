//! The channel sidebar: an account/team switcher in the header, the list below.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use mattermost_api::models::{Channel, ChannelType, Presence, User};

use crate::avatars::Avatars;
use crate::state::SharedState;

/// The channel list, pane one.
pub struct ChannelSidebar {
    pub widget: adw::ToolbarView,
    list: gtk::ListBox,
    title: gtk::Label,
    switcher: Switcher,
    updating: Rc<RefCell<bool>>,
}

impl ChannelSidebar {
    pub fn new(
        on_select: impl Fn(String) + 'static,
        on_select_team: impl Fn(String) + 'static,
        on_search: impl Fn(String) + 'static,
        on_status: impl Fn(String) + 'static,
        dock: &gtk::Widget,
    ) -> Self {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        list.add_css_class("navigation-sidebar");
        list.add_css_class("channel-list");

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

        let switcher = Switcher::new(on_select_team, on_status);

        let header = adw::HeaderBar::builder()
            .title_widget(&title)
            .show_end_title_buttons(false)
            .build();
        header.pack_start(&switcher.button);

        // A search bar rather than a dialog: it belongs to the list it filters
        // into, and Escape puts it away without losing your place.
        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Search messages")
            .hexpand(true)
            .build();
        let search_bar = gtk::SearchBar::builder()
            .child(&search_entry)
            .key_capture_widget(&header)
            .build();
        let search_button = gtk::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search messages")
            .build();
        search_button.add_css_class("flat");
        search_button
            .bind_property("active", &search_bar, "search-mode-enabled")
            .bidirectional()
            .sync_create()
            .build();
        header.pack_end(&search_button);

        // The main menu, where the things you do to the *list* live rather
        // than to any one channel.
        let menu = gtk::gio::Menu::new();
        let channels_section = gtk::gio::Menu::new();
        channels_section.append(Some("Jump to…"), Some("win.quick-switch"));
        channels_section.append(Some("New Channel…"), Some("win.new-channel"));
        channels_section.append(Some("Browse Channels…"), Some("win.browse-channels"));
        menu.append_section(None, &channels_section);
        let account_section = gtk::gio::Menu::new();
        account_section.append(Some("Edit Profile…"), Some("win.edit-profile"));
        account_section.append(Some("Set a Status…"), Some("win.custom-status"));
        account_section.append(Some("Notifications…"), Some("win.notification-settings"));
        account_section.append(Some("Sign Out"), Some("win.sign-out"));
        menu.append_section(None, &account_section);

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Main menu")
            .menu_model(&menu)
            .primary(true)
            .build();
        menu_button.add_css_class("flat");
        header.pack_end(&menu_button);

        // On activate, not on every keystroke: a post search is a round trip
        // to the server, and searching per character would be a request per
        // character.
        search_entry.connect_activate(move |entry| {
            let terms = entry.text().trim().to_string();
            if !terms.is_empty() {
                on_search(terms);
            }
        });

        // The call dock is the toolbar view's bottom bar rather than another
        // child of a box: that reserves its space, draws the separator, and
        // animates the reveal, none of which a plain box would do.
        let widget = adw::ToolbarView::builder()
            .bottom_bar_style(adw::ToolbarStyle::RaisedBorder)
            .reveal_bottom_bars(false)
            .build();
        widget.add_top_bar(&header);
        widget.add_top_bar(&search_bar);
        widget.set_content(Some(&scroller));
        widget.add_bottom_bar(dock);

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
            switcher,
            updating,
        }
    }

    pub fn refresh(&self, state: &SharedState, avatars: &Avatars) {
        self.switcher.refresh(state, avatars);
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
                let title = st.channel_title(&channel);
                // A DM is a person, so it gets that person's face with the
                // presence badge, exactly like a message row.
                let icon = channel
                    .dm_teammate_id(&st.me.id)
                    .map(|user_id| dm_avatar(avatars, user_id, &title, st.presence(user_id)));
                let row = channel_row(&channel, &title, icon, avatars, &st);
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

/// The status dot drawn over your own avatar.
fn presence_dot(size: i32) -> gtk::Box {
    let dot = gtk::Box::builder()
        .width_request(size)
        .height_request(size)
        .halign(gtk::Align::End)
        .valign(gtk::Align::End)
        .build();
    dot.add_css_class("presence-badge");
    dot
}

fn category_header(name: &str) -> gtk::ListBoxRow {
    let label = gtk::Label::builder()
        .label(name)
        .xalign(0.0)
        .hexpand(true)
        .build();
    label.add_css_class("dim-label");

    // A box, not a bare label: every row in this list is styled through its
    // child, so a header without one would miss the shared indentation.
    let row_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .build();
    row_box.add_css_class("channel-category");
    row_box.append(&label);

    gtk::ListBoxRow::builder()
        .child(&row_box)
        .selectable(false)
        .activatable(false)
        .build()
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
    icon: Option<gtk::Widget>,
    avatars: &Avatars,
    state: &crate::state::AppState,
) -> gtk::ListBoxRow {
    let unread = state.unread(&channel.id);
    let in_call = state.active_calls.get(&channel.id);
    let has_draft = state.drafts.contains_key(&channel.id);

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
    if let Some(people) = in_call {
        row_box.append(&call_badge(people, avatars, state));
    }

    // Something unsent here. Shown even on a muted channel: it is your own
    // text waiting, not someone else's noise.
    if has_draft {
        let pencil = gtk::Image::from_icon_name("document-edit-symbolic");
        pencil.set_pixel_size(12);
        pencil.add_css_class("dim-label");
        pencil.set_tooltip_text(Some("You have an unsent message here"));
        row_box.append(&pencil);
    }

    if unread.mentions > 0 {
        let badge = gtk::Label::new(Some(&unread.mentions.to_string()));
        badge.add_css_class("mention-badge");
        if unread.urgent {
            badge.add_css_class("urgent");
        }
        row_box.append(&badge);
    } else if unread.is_unread() && !unread.muted {
        // Unread without a mention is a dot, the way Fractal marks a room:
        // enough to notice, not enough to demand a number be read.
        let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
        dot.add_css_class("unread-dot");
        row_box.append(&dot);
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

/// Who is in the call, the way Slack marks a channel: a few faces and the
/// count. Faces beat an icon here — the reason to join is usually who is there.
fn call_badge(people: &[String], avatars: &Avatars, state: &crate::state::AppState) -> gtk::Widget {
    /// Beyond this the faces are unreadable at 16px and the count carries it.
    const FACES: usize = 3;

    let badge = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(2)
        .valign(gtk::Align::Center)
        .build();
    badge.add_css_class("call-badge");

    let headset = gtk::Image::from_icon_name("audio-headphones-symbolic");
    headset.set_pixel_size(12);
    badge.append(&headset);

    for user_id in people.iter().take(FACES) {
        let name = state
            .users
            .get(user_id)
            .map(|u| u.display_name(state.teammate_name_display()))
            .unwrap_or_default();
        let avatar = adw::Avatar::builder().size(16).build();
        avatars.apply(&avatar, user_id, &name);
        badge.append(&avatar);
    }

    // The count is only news once it exceeds the faces already shown.
    if people.len() > FACES {
        badge.append(&gtk::Label::new(Some(&format!(
            "+{}",
            people.len() - FACES
        ))));
    }

    badge.set_tooltip_text(Some(&match people.len() {
        0 => "A call is starting".to_string(),
        1 => "1 person is in a call".to_string(),
        n => format!("{n} people are in a call"),
    }));
    badge.upcast()
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

/// The round avatar in the sidebar header and what drops out of it: who you
/// are signed in as, and the teams to switch between. This is where the old
/// 68px team rail went — Fractal puts its account switcher in exactly this
/// spot, and a rail costs a permanent column to say what a popover says on
/// demand.
struct Switcher {
    button: gtk::MenuButton,
    avatar: adw::Avatar,
    avatar_dot: gtk::Box,
    account_avatar: adw::Avatar,
    account_dot: gtk::Box,
    name: gtk::Label,
    username: gtk::Label,
    teams: gtk::ListBox,
}

impl Switcher {
    fn new(
        on_select_team: impl Fn(String) + 'static,
        on_status: impl Fn(String) + 'static,
    ) -> Self {
        let avatar = adw::Avatar::builder().size(24).build();
        let avatar_dot = presence_dot(8);
        let avatar_stack = gtk::Overlay::builder().child(&avatar).build();
        avatar_stack.add_overlay(&avatar_dot);

        let button = gtk::MenuButton::builder()
            .child(&avatar_stack)
            .tooltip_text("Account and teams")
            .build();
        button.add_css_class("image-button");
        button.add_css_class("circular");
        button.add_css_class("flat");

        let account_avatar = adw::Avatar::builder().size(40).build();
        let account_dot = presence_dot(12);
        let account_stack = gtk::Overlay::builder().child(&account_avatar).build();
        account_stack.add_overlay(&account_dot);
        let name = gtk::Label::builder().xalign(0.0).build();
        name.add_css_class("heading");
        let username = gtk::Label::builder().xalign(0.0).build();
        username.add_css_class("caption");
        username.add_css_class("dim-label");

        let labels = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .build();
        labels.append(&name);
        labels.append(&username);

        let account = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .build();
        account.append(&account_stack);
        account.append(&labels);

        // Setting your own status belongs with your own name, which is here.
        let statuses = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .homogeneous(true)
            .margin_start(6)
            .margin_end(6)
            .build();
        let on_status = Rc::new(on_status);
        for (label, value, css) in [
            ("Online", "online", "presence-online"),
            ("Away", "away", "presence-away"),
            ("Do not disturb", "dnd", "presence-dnd"),
            ("Offline", "offline", "presence-offline"),
        ] {
            let dot = gtk::Label::new(Some("●"));
            dot.add_css_class("presence-dot");
            dot.add_css_class(css);
            let button = gtk::Button::builder()
                .child(&dot)
                .tooltip_text(label)
                .build();
            button.add_css_class("flat");
            button.connect_clicked({
                let on_status = on_status.clone();
                let button_parent = button.clone();
                move |_| {
                    on_status(value.to_string());
                    // Close the popover the button lives in, so the choice
                    // registers as made.
                    if let Some(popover) = button_parent
                        .ancestor(gtk::Popover::static_type())
                        .and_downcast::<gtk::Popover>()
                    {
                        popover.popdown();
                    }
                }
            });
            statuses.append(&button);
        }

        let teams = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        teams.add_css_class("navigation-sidebar");

        // Row *activation*, not selection: selection also fires while we
        // rebuild the list, and re-selecting the current team would reload it.
        teams.connect_row_activated({
            let button = button.clone();
            move |_, row| {
                if let Some(id) = unsafe { row.data::<String>("team-id") } {
                    button.popdown();
                    on_select_team(unsafe { id.as_ref() }.clone());
                }
            }
        });

        let teams_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(320)
            .child(&teams)
            .build();

        let heading = gtk::Label::builder()
            .label("TEAMS")
            .xalign(0.0)
            .margin_start(12)
            .build();
        heading.add_css_class("caption-heading");
        heading.add_css_class("dim-label");

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .width_request(260)
            .build();
        content.append(&account);
        content.append(&statuses);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&heading);
        content.append(&teams_scroller);

        button.set_popover(Some(&gtk::Popover::builder().child(&content).build()));

        Switcher {
            button,
            avatar,
            avatar_dot,
            account_avatar,
            account_dot,
            name,
            username,
            teams,
        }
    }

    fn refresh(&self, state: &SharedState, avatars: &Avatars) {
        let st = state.borrow();
        let me: &User = &st.me;
        let display = me.display_name(st.teammate_name_display());
        avatars.apply(&self.avatar, &me.id, &display);
        avatars.apply(&self.account_avatar, &me.id, &display);
        // The dot on your own face is the only place the chosen status shows.
        let presence = st.presence(&me.id);
        for dot in [&self.avatar_dot, &self.account_dot] {
            for name in [
                "presence-online",
                "presence-away",
                "presence-dnd",
                "presence-offline",
            ] {
                dot.remove_css_class(name);
            }
            dot.add_css_class(super::profile::presence_class(presence));
        }
        self.name.set_text(&display);
        self.username.set_text(&format!("@{}", me.username));

        while let Some(child) = self.teams.first_child() {
            self.teams.remove(&child);
        }
        for team in st.teams.iter().filter(|t| t.delete_at == 0) {
            let avatar = adw::Avatar::builder()
                .size(24)
                .text(&team.display_name)
                .show_initials(true)
                .build();
            let label = gtk::Label::builder()
                .label(&team.display_name)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let row_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .build();
            row_box.append(&avatar);
            row_box.append(&label);

            let row = gtk::ListBoxRow::builder().child(&row_box).build();
            unsafe { row.set_data("team-id", team.id.clone()) };
            self.teams.append(&row);

            if st.current_team.as_deref() == Some(team.id.as_str()) {
                self.teams.select_row(Some(&row));
            }
        }
    }
}
