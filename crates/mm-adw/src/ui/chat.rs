//! Pane 3: the conversation — header, call banner, message list, composer.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use mattermost_api::models::Millis;

use crate::avatars::Avatars;
use crate::state::SharedState;
use crate::ui::message::{self, MessageActions, RowOptions};

/// Everything the conversation pane reports back to the action loop. A struct
/// rather than nine positional closures: at that count the compiler stops
/// catching a swapped pair, and the caller cannot see which is which.
pub struct ChatCallbacks {
    pub on_send: Box<dyn Fn(String)>,
    pub on_typing: Box<dyn Fn(bool)>,
    pub on_attach: Box<dyn Fn()>,
    pub on_files: Box<dyn Fn(Vec<std::path::PathBuf>)>,
    pub on_complete: Box<dyn Fn(Option<super::autocomplete::Query>)>,
    pub on_schedule: Box<dyn Fn()>,
    pub on_agent: Box<dyn Fn()>,
    pub on_call: Box<dyn Fn()>,
    pub on_inbox: Box<dyn Fn()>,
}

pub struct ChatView {
    pub widget: adw::ToolbarView,
    title: gtk::Label,
    subtitle: gtk::Label,
    messages: gtk::Box,
    scroller: gtk::ScrolledWindow,
    entry: gtk::TextView,
    call_button: gtk::Button,
    inbox_button: gtk::Button,
    complete: Rc<super::autocomplete::Autocomplete>,
    /// The priority chosen for the next message, as a stateful action.
    priority_action: gtk::gio::SimpleAction,
    agent_button: gtk::MenuButton,
    /// Kept so the menu can be rebuilt without losing the fixed entries.
    agent_actions: Rc<RefCell<Vec<gtk::gio::SimpleAction>>>,
    inbox_badge: gtk::Label,
    call_banner: gtk::Box,
    call_banner_label: gtk::Label,
    join_button: gtk::Button,
    stack: gtk::Stack,
    /// Shown instead of an empty feed while the first page is in flight, so a
    /// slow channel reads as loading rather than as empty.
    loading: Rc<RefCell<bool>>,
    connection: adw::Banner,
    typing: gtk::Label,
    attachments: gtk::Box,
    edit_banner: adw::Banner,
    /// The post being edited, when the composer is in edit mode.
    editing: Rc<RefCell<Option<String>>>,
    /// Set while a draft is being put back, so the change it causes is not
    /// mistaken for the user typing.
    restoring: Rc<RefCell<bool>>,
    /// False while the reader is scrolled up in history, so live messages do
    /// not yank them back to the bottom.
    pinned_to_bottom: Rc<RefCell<bool>>,
}

impl ChatView {
    pub fn new(callbacks: ChatCallbacks) -> Self {
        let ChatCallbacks {
            on_send,
            on_typing,
            on_attach,
            on_files,
            on_complete,
            on_schedule,
            on_agent,
            on_call,
            on_inbox,
        } = callbacks;
        let title = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        title.add_css_class("heading");
        let subtitle = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .visible(false)
            .build();
        subtitle.add_css_class("channel-header-subtitle");
        subtitle.add_css_class("dim-label");

        let title_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .build();
        title_box.append(&title);
        title_box.append(&subtitle);

        let call_button = gtk::Button::builder()
            .icon_name("call-start-symbolic")
            .tooltip_text("Start or join a call")
            .sensitive(false)
            .build();
        call_button.add_css_class("flat");
        let on_call = std::rc::Rc::new(on_call);
        call_button.connect_clicked({
            let on_call = on_call.clone();
            move |_| on_call()
        });

        // The inbox button carries its own count, the way a mail client's does:
        // the number is the reason to click it.
        let inbox_badge = gtk::Label::builder().visible(false).build();
        inbox_badge.add_css_class("mention-badge");
        inbox_badge.add_css_class("button-badge");
        let inbox_icon = gtk::Image::from_icon_name("mail-unread-symbolic");
        let inbox_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();
        inbox_content.append(&inbox_icon);
        inbox_content.append(&inbox_badge);
        let inbox_button = gtk::Button::builder()
            .child(&inbox_content)
            .tooltip_text("Mentions and threads")
            .build();
        inbox_button.add_css_class("flat");
        inbox_button.connect_clicked(move |_| on_inbox());

        let header = adw::HeaderBar::builder().title_widget(&title_box).build();
        // Only there when a server actually has an agent to ask. A menu
        // rather than a button because there are two different things to want
        // from an agent: a summary of here, or a conversation with it.
        let agent_button = gtk::MenuButton::builder()
            .icon_name("bot-symbolic")
            .tooltip_text("Agents")
            .visible(false)
            .build();
        agent_button.add_css_class("flat");
        let agent_actions = gtk::gio::SimpleActionGroup::new();
        let catch_up = gtk::gio::SimpleAction::new("catch-up", None);
        catch_up.connect_activate(move |_, _| on_agent());
        agent_actions.add_action(&catch_up);
        agent_button.insert_action_group("agent", Some(&agent_actions));

        // What you do to *this* channel, as opposed to the message under the
        // pointer or the list in the sidebar.
        let channel_menu = gtk::gio::Menu::new();
        channel_menu.append(Some("Channel Details…"), Some("win.edit-channel"));
        channel_menu.append(Some("Members…"), Some("win.channel-members"));
        channel_menu.append(Some("Bookmarks…"), Some("win.channel-bookmarks"));
        channel_menu.append(Some("Pinned Messages"), Some("win.pinned-posts"));
        channel_menu.append(Some("Notifications…"), Some("win.channel-notifications"));
        channel_menu.append(Some("Leave Channel"), Some("win.leave-channel"));
        channel_menu.append(Some("Archive Channel"), Some("win.archive-channel"));
        let channel_button = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .tooltip_text("Channel menu")
            .menu_model(&channel_menu)
            .build();
        channel_button.add_css_class("flat");

        header.pack_end(&channel_button);
        header.pack_end(&call_button);
        header.pack_end(&agent_button);
        header.pack_end(&inbox_button);

        // --- call banner
        let call_banner_label = gtk::Label::builder().xalign(0.0).hexpand(true).build();
        let call_banner = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .visible(false)
            .build();
        call_banner.add_css_class("call-banner");
        call_banner.append(&gtk::Image::from_icon_name("call-start-symbolic"));
        call_banner.append(&call_banner_label);

        // The banner is where you notice a call, so it is where joining it
        // belongs — the header button is for starting one.
        let join_button = gtk::Button::with_label("Join");
        join_button.add_css_class("pill");
        join_button.add_css_class("call-banner-join");
        join_button.connect_clicked(move |_| on_call());
        call_banner.append(&join_button);

        // --- messages
        let messages = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .valign(gtk::Align::End)
            .vexpand(true)
            .build();

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&messages)
            .build();

        let pinned_to_bottom = Rc::new(RefCell::new(true));
        {
            let pinned = pinned_to_bottom.clone();
            scroller.vadjustment().connect_value_changed(move |adj| {
                let at_bottom = adj.value() + adj.page_size() >= adj.upper() - 32.0;
                *pinned.borrow_mut() = at_bottom;
            });
        }

        // --- composer
        let entry = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .accepts_tab(false)
            .top_margin(8)
            .bottom_margin(8)
            .left_margin(8)
            .right_margin(8)
            .build();

        let entry_frame = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(40)
            .max_content_height(160)
            .propagate_natural_height(true)
            .child(&entry)
            .build();
        entry_frame.add_css_class("card");

        let send = gtk::Button::builder()
            .icon_name("document-send-symbolic")
            .tooltip_text("Send  (Enter)")
            .valign(gtk::Align::End)
            .build();
        send.add_css_class("suggested-action");
        send.add_css_class("circular");

        let priority_menu = gtk::gio::Menu::new();
        for (label, value) in [
            ("Standard", "standard"),
            ("Important", "important"),
            ("Urgent", "urgent"),
        ] {
            priority_menu.append(Some(label), Some(&format!("composer.priority::{value}")));
        }
        let priority = gtk::MenuButton::builder()
            .icon_name("emblem-important-symbolic")
            .tooltip_text("Message priority")
            .menu_model(&priority_menu)
            .valign(gtk::Align::End)
            .build();
        priority.add_css_class("flat");
        priority.add_css_class("circular");

        let priority_action = gtk::gio::SimpleAction::new_stateful(
            "priority",
            Some(&String::static_variant_type()),
            &"standard".to_variant(),
        );
        priority_action.connect_activate({
            let priority = priority.clone();
            move |action, target| {
                let Some(value) = target.and_then(|t| t.get::<String>()) else {
                    return;
                };
                // The button carries the current choice: a priority you set
                // and cannot see is one you will send by accident.
                priority.set_css_classes(&["flat", "circular"]);
                match value.as_str() {
                    "important" => priority.add_css_class("accent"),
                    "urgent" => priority.add_css_class("error"),
                    _ => {}
                }
                action.set_state(&value.to_variant());
            }
        });
        let composer_actions = gtk::gio::SimpleActionGroup::new();
        composer_actions.add_action(&priority_action);

        let attach = gtk::Button::builder()
            .icon_name("mail-attachment-symbolic")
            .tooltip_text("Attach a file")
            .valign(gtk::Align::End)
            .build();
        attach.add_css_class("flat");
        attach.add_css_class("circular");
        attach.connect_clicked(move |_| on_attach());

        let composer = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_top(6)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        composer.insert_action_group("composer", Some(&composer_actions));
        let schedule = gtk::Button::builder()
            .icon_name("alarm-symbolic")
            .tooltip_text("Send later")
            .valign(gtk::Align::End)
            .build();
        schedule.add_css_class("flat");
        schedule.add_css_class("circular");
        schedule.connect_clicked(move |_| on_schedule());

        composer.append(&attach);
        composer.append(&priority);
        composer.append(&schedule);
        composer.append(&entry_frame);
        composer.append(&send);

        let on_send = Rc::new(on_send);
        let submit = {
            let entry = entry.clone();
            let on_send = on_send.clone();
            move || {
                let buffer = entry.buffer();
                let (start, end) = buffer.bounds();
                let text = buffer.text(&start, &end, false).trim().to_string();
                if text.is_empty() {
                    return;
                }
                buffer.set_text("");
                on_send(text);
            }
        };

        send.connect_clicked({
            let submit = submit.clone();
            move |_| submit()
        });

        // Enter sends; Shift+Enter inserts a newline.
        // The completion popover has first refusal on keys: Enter picks a
        // candidate when the list is open, and only sends when it is not.
        let complete = Rc::new(super::autocomplete::Autocomplete::new(&entry, {
            let on_complete = Rc::new(on_complete);
            move |query| on_complete(query)
        }));

        // Ctrl+V with an image on the clipboard attaches it. Pasting a
        // screenshot is how most images get into a chat, and the alternative
        // is saving it to disk first for no reason.
        let on_files = Rc::new(on_files);
        let paste = gtk::EventControllerKey::new();
        paste.connect_key_pressed({
            let on_files = on_files.clone();
            move |controller, key, _, modifier| {
                let ctrl_v = key == gtk::gdk::Key::v
                    && modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK);
                if !ctrl_v {
                    return glib::Propagation::Proceed;
                }
                let Some(clipboard) = controller.widget().map(|w| w.clipboard()) else {
                    return glib::Propagation::Proceed;
                };
                // Only claim the keystroke when there really is an image;
                // otherwise the text paste has to go through untouched.
                if !clipboard
                    .formats()
                    .contains_type(gtk::gdk::Texture::static_type())
                {
                    return glib::Propagation::Proceed;
                }

                let on_files = on_files.clone();
                clipboard.read_texture_async(gtk::gio::Cancellable::NONE, move |result| {
                    let Ok(Some(texture)) = result else { return };
                    // The upload path takes paths, so the pasted image lands
                    // in a temp file that the OS cleans up.
                    let path = std::env::temp_dir()
                        .join(format!("mm-adw-paste-{}.png", glib::monotonic_time()));
                    if let Err(e) = texture.save_to_png(&path) {
                        tracing::warn!(error = %e, "could not save the pasted image");
                        return;
                    }
                    on_files(vec![path]);
                });
                glib::Propagation::Stop
            }
        });
        entry.add_controller(paste);

        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed({
            let complete = complete.clone();
            let submit = submit.clone();
            move |_, key, _, modifier| {
                if complete.handle_key(key) {
                    return glib::Propagation::Stop;
                }
                let enter = matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter);
                if enter && !modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                    submit();
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
        });
        entry.add_controller(keys);

        let restoring = Rc::new(RefCell::new(false));
        entry.buffer().connect_changed({
            let restoring = restoring.clone();
            move |buffer| {
                // Restoring a draft is not typing, and must not be reported as
                // either a keystroke or a fresh draft.
                if *restoring.borrow() {
                    return;
                }
                on_typing(buffer.char_count() > 0);
            }
        });

        // Uploaded files wait here until a message carries them.
        let attachments = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_start(14)
            .margin_end(14)
            .margin_bottom(4)
            .visible(false)
            .build();

        let editing: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        // Editing is a mode, and a mode you cannot see is a trap: the banner
        // says so and offers the way out.
        let edit_banner = adw::Banner::builder()
            .title("Editing a message")
            .button_label("Cancel")
            .revealed(false)
            .build();
        edit_banner.connect_button_clicked({
            let editing = editing.clone();
            let entry = entry.clone();
            move |banner| {
                *editing.borrow_mut() = None;
                banner.set_revealed(false);
                entry.buffer().set_text("");
            }
        });

        // Sits between the feed and the composer, reserving no space when
        // empty: a line that appears and disappears must not shove the
        // conversation up and down.
        let typing = gtk::Label::builder()
            .xalign(0.0)
            .margin_start(14)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .visible(false)
            .build();
        typing.add_css_class("typing-line");
        typing.add_css_class("dim-label");

        // Losing the socket is a state of the whole window, not an event, so
        // it gets a banner that stays up rather than a toast that scrolls by.
        let connection = adw::Banner::builder().revealed(false).build();

        let conversation = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        conversation.append(&connection);
        conversation.append(&call_banner);
        conversation.append(&scroller);
        conversation.append(&edit_banner);
        conversation.append(&typing);
        conversation.append(&attachments);
        conversation.append(&composer);

        let placeholder = adw::StatusPage::builder()
            .icon_name("chat-message-new-symbolic")
            .title("No channel selected")
            .description("Pick a channel from the sidebar to start reading.")
            .build();

        let spinner = gtk::Spinner::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .width_request(32)
            .height_request(32)
            .build();
        spinner.start();

        let stack = gtk::Stack::new();
        stack.add_named(&placeholder, Some("empty"));
        stack.add_named(&spinner, Some("loading"));
        stack.add_named(&conversation, Some("conversation"));
        stack.set_visible_child_name("empty");

        // Dropping files anywhere over the conversation attaches them. The
        // target is the whole pane rather than the composer: aiming at a
        // one-line text box is a needlessly precise thing to ask of a drag.
        let drop = gtk::DropTarget::new(
            gtk::gdk::FileList::static_type(),
            gtk::gdk::DragAction::COPY,
        );
        drop.connect_drop({
            let on_files = on_files.clone();
            move |_, value, _, _| {
                let Ok(files) = value.get::<gtk::gdk::FileList>() else {
                    return false;
                };
                let paths: Vec<std::path::PathBuf> =
                    files.files().iter().filter_map(|f| f.path()).collect();
                if paths.is_empty() {
                    return false;
                }
                on_files(paths);
                true
            }
        });
        conversation.add_controller(drop);

        // A toolbar view rather than a plain box: on a narrow window the call
        // dock moves in here as a bottom bar, since the sidebar it normally
        // lives in is off-screen.
        let widget = adw::ToolbarView::builder()
            .bottom_bar_style(adw::ToolbarStyle::RaisedBorder)
            .reveal_bottom_bars(false)
            .build();
        widget.add_top_bar(&header);
        widget.set_content(Some(&stack));

        ChatView {
            widget,
            title,
            subtitle,
            messages,
            scroller,
            entry,
            call_button,
            inbox_button,
            inbox_badge,
            call_banner,
            call_banner_label,
            join_button,
            stack,
            complete,
            priority_action,
            agent_button,
            agent_actions: Rc::new(RefCell::new(vec![catch_up.clone()])),
            typing,
            attachments,
            edit_banner,
            editing,
            restoring,
            loading: Rc::new(RefCell::new(false)),
            connection,
            pinned_to_bottom,
        }
    }

    /// Marks the first page of a channel as in flight. Only matters while
    /// there is nothing to show: a reload over existing messages should leave
    /// them on screen rather than blank the pane.
    pub fn set_loading(&self, loading: bool) {
        *self.loading.borrow_mut() = loading;
    }

    pub fn set_typing(&self, names: &[String]) {
        let text = match names {
            [] => String::new(),
            [one] => format!("{one} is typing…"),
            [one, two] => format!("{one} and {two} are typing…"),
            [one, two, ..] => format!("{one}, {two} and others are typing…"),
        };
        self.typing.set_visible(!text.is_empty());
        self.typing.set_text(&text);
    }

    /// `None` means connected. Anything else is shown until it is cleared.
    pub fn set_connection_problem(&self, problem: Option<&str>) {
        match problem {
            Some(text) => {
                self.connection.set_title(text);
                self.connection.set_revealed(true);
            }
            None => self.connection.set_revealed(false),
        }
    }

    /// Rebuilds the agent menu: one entry to summarise this channel, and one
    /// per bot to go and talk to it.
    pub fn set_agents(&self, bots: &[(String, String)], on_open: impl Fn(String) + 'static) {
        self.agent_button.set_visible(!bots.is_empty());
        if bots.is_empty() {
            return;
        }

        let menu = gtk::gio::Menu::new();
        menu.append(Some("Catch me up"), Some("agent.catch-up"));

        let chats = gtk::gio::Menu::new();
        let on_open = Rc::new(on_open);
        let group = gtk::gio::SimpleActionGroup::new();
        for (index, (id, name)) in bots.iter().enumerate() {
            let action_name = format!("chat-{index}");
            let action = gtk::gio::SimpleAction::new(&action_name, None);
            action.connect_activate({
                let on_open = on_open.clone();
                let id = id.clone();
                move |_, _| on_open(id.clone())
            });
            group.add_action(&action);
            chats.append(Some(name), Some(&format!("agent.{action_name}")));
        }
        menu.append_section(None, &chats);

        // Rebuilding replaces the action group, so the fixed entries have to
        // be carried over or "Catch me up" stops working after the first
        // refresh.
        for action in self.agent_actions.borrow().iter() {
            group.add_action(action);
        }
        self.agent_button.set_menu_model(Some(&menu));
        self.agent_button.insert_action_group("agent", Some(&group));
    }

    /// Redraws the row of files waiting to go out with the next message.
    pub fn set_attachments(
        &self,
        files: &[(String, String)],
        on_remove: impl Fn(String) + 'static,
    ) {
        while let Some(child) = self.attachments.first_child() {
            self.attachments.remove(&child);
        }
        self.attachments.set_visible(!files.is_empty());

        let on_remove = Rc::new(on_remove);
        for (id, name) in files {
            let label = gtk::Label::builder()
                .label(name)
                .ellipsize(gtk::pango::EllipsizeMode::Middle)
                .max_width_chars(24)
                .build();
            let remove = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .tooltip_text("Remove")
                .build();
            remove.add_css_class("flat");
            remove.add_css_class("circular");
            remove.connect_clicked({
                let on_remove = on_remove.clone();
                let id = id.clone();
                move |_| on_remove(id.clone())
            });

            let chip = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(4)
                .build();
            chip.add_css_class("attachment-chip");
            chip.append(&gtk::Image::from_icon_name("mail-attachment-symbolic"));
            chip.append(&label);
            chip.append(&remove);
            self.attachments.append(&chip);
        }
    }

    /// A file being uploaded shows as a chip that is not yet removable.
    pub fn set_uploading(&self, count: usize) {
        if count == 0 {
            return;
        }
        self.attachments.set_visible(true);
        let spinner = gtk::Spinner::new();
        spinner.start();
        let chip = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        chip.add_css_class("attachment-chip");
        chip.append(&spinner);
        chip.append(&gtk::Label::new(Some(&match count {
            1 => "Uploading…".to_string(),
            n => format!("Uploading {n} files…"),
        })));
        self.attachments.append(&chip);
    }

    /// Puts the composer into edit mode for an existing post.
    ///
    /// Editing reuses the composer rather than opening a second one: there is
    /// only ever one message being written at a time, and a separate box would
    /// be a second place to lose text in.
    pub fn begin_edit(&self, post_id: &str, text: &str) {
        *self.editing.borrow_mut() = Some(post_id.to_string());
        self.set_composer_text(text);
        self.edit_banner.set_revealed(true);
        self.entry.grab_focus();
    }

    /// Leaves edit mode, clearing the composer.
    pub fn end_edit(&self) {
        *self.editing.borrow_mut() = None;
        self.edit_banner.set_revealed(false);
        self.set_composer_text("");
    }

    /// The post being edited, if any.
    pub fn editing(&self) -> Option<String> {
        self.editing.borrow().clone()
    }

    /// Answers an outstanding completion query.
    pub fn set_completions(&self, items: Vec<(String, String, String)>) {
        self.complete.set_candidates(items);
    }

    /// The priority for the message being written: "", "important" or
    /// "urgent". Empty means standard, which is what the server expects.
    pub fn priority(&self) -> String {
        match self.priority_action.state().and_then(|s| s.get::<String>()) {
            Some(value) if value != "standard" => value,
            _ => String::new(),
        }
    }

    /// Back to standard once a message has gone out. Priority is per message,
    /// and a sticky "urgent" would quietly escalate everything after it.
    pub fn reset_priority(&self) {
        self.priority_action.set_state(&"standard".to_variant());
    }

    /// What is in the composer right now.
    pub fn composer_text(&self) -> String {
        let buffer = self.entry.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }

    /// Puts a draft back. Setting the buffer fires `changed`, which would
    /// otherwise be read as the user typing and save the draft straight back —
    /// hence the guard.
    pub fn set_composer_text(&self, text: &str) {
        if self.composer_text() == text {
            return;
        }
        *self.restoring.borrow_mut() = true;
        self.entry.buffer().set_text(text);
        // Put the cursor where they left off, not at the front.
        let buffer = self.entry.buffer();
        buffer.place_cursor(&buffer.end_iter());
        *self.restoring.borrow_mut() = false;
    }

    pub fn focus_composer(&self) {
        self.entry.grab_focus();
    }

    pub fn set_calls_available(&self, available: bool, reason: Option<&str>) {
        self.call_button.set_sensitive(available);
        if let Some(reason) = reason {
            self.call_button.set_tooltip_text(Some(reason));
        }
    }

    /// Reflects our own membership. Everything you can *do* inside a call is
    /// in the dock now; the header only starts, joins or ends one.
    pub fn set_in_call(&self, in_call: bool, ongoing: bool) {
        self.call_button.set_icon_name(if in_call {
            "call-stop-symbolic"
        } else {
            "call-start-symbolic"
        });
        self.call_button
            .set_tooltip_text(Some(match (in_call, ongoing) {
                (true, _) => "Leave the call",
                // A call is running here and we are outside it: the button
                // joins, which is not something an icon alone ever says.
                (false, true) => "Join the call",
                (false, false) => "Start a call",
            }));
        set_active(&self.call_button, in_call, "destructive-action");
        if in_call {
            self.call_button.set_sensitive(true);
        } else {
            // Accent while a call is waiting for you, so the header agrees
            // with the banner instead of looking idle.
            set_active(&self.call_button, ongoing, "suggested-action");
        }
        self.join_button.set_visible(!in_call);
    }

    pub fn set_call_in_progress(&self, participants: Option<usize>) {
        match participants {
            Some(n) => {
                self.call_banner_label.set_text(&match n {
                    0 => "A call is starting".to_string(),
                    1 => "1 person is in a call".to_string(),
                    n => format!("{n} people are in a call"),
                });
                self.call_banner.set_visible(true);
            }
            None => self.call_banner.set_visible(false),
        }
    }

    fn set_inbox_count(&self, count: i64) {
        self.inbox_badge.set_visible(count > 0);
        self.inbox_badge.set_text(&count.to_string());
        self.inbox_button.set_tooltip_text(Some(&if count > 0 {
            format!("{count} unread — mentions and threads")
        } else {
            "Mentions and threads".to_string()
        }));
    }

    /// Redraws the whole feed for the current channel.
    pub fn refresh(&self, state: &SharedState, avatars: &Avatars, actions: &MessageActions) {
        let st = state.borrow();
        let inbox_count = (st
            .mentions
            .len()
            .min(99)
            .max(st.unread_threads().max(0) as usize) as i64)
            + st.reaction_unread;
        let Some(channel_id) = st.current_channel.clone() else {
            drop(st);
            self.set_inbox_count(inbox_count);
            self.stack.set_visible_child_name("empty");
            return;
        };
        let Some(channel) = st.channel(&channel_id).cloned() else {
            drop(st);
            self.set_inbox_count(inbox_count);
            self.stack.set_visible_child_name("empty");
            return;
        };

        // A channel with no posts *yet* and a fetch in flight is loading; one
        // with no posts and nothing in flight is genuinely empty.
        let waiting =
            *self.loading.borrow() && st.feeds.get(&channel_id).is_none_or(|f| f.posts.is_empty());
        self.stack
            .set_visible_child_name(if waiting { "loading" } else { "conversation" });
        self.title.set_text(&st.channel_title(&channel));
        let header_line = channel.header.lines().next().unwrap_or("").to_string();
        self.subtitle.set_text(&header_line);
        self.subtitle.set_visible(!header_line.is_empty());

        let empty = crate::state::ChannelFeed::default();
        let feed = st.feeds.get(&channel_id).unwrap_or(&empty);
        let posts = feed.posts.clone();
        let at_latest = feed.at_latest || posts.is_empty();
        let at_oldest = feed.at_oldest;
        let channel_title = st.channel_title(&channel);
        let crt = st.crt_enabled;
        drop(st);

        self.set_inbox_count(inbox_count);

        while let Some(child) = self.messages.first_child() {
            self.messages.remove(&child);
        }

        // Only claim "this is the start" when we actually hold the oldest
        // block; otherwise there is simply more history we have not paged in.
        if at_oldest && !posts.is_empty() {
            let start = gtk::Label::builder()
                .label(format!("This is the beginning of {channel_title}"))
                .xalign(0.0)
                .wrap(true)
                .margin_bottom(8)
                .build();
            start.add_css_class("dim-label");
            self.messages.append(&start);
        }

        if posts.is_empty() {
            self.messages.append(
                &adw::StatusPage::builder()
                    .icon_name("chat-message-new-symbolic")
                    .title("No messages yet")
                    .description("Say something to get started.")
                    .vexpand(true)
                    .build(),
            );
        }

        let mut last_author: Option<String> = None;
        let mut last_at: Millis = 0;
        let mut last_day: Option<String> = None;

        for post in &posts {
            if post.is_deleted() {
                continue;
            }
            // Belt and braces: under CRT a reply must never reach the channel
            // feed, and a server that sends one anyway should not break the
            // reading order.
            if crt && post.is_reply() {
                continue;
            }

            let day = message::format_day(post.create_at);
            if last_day.as_deref() != Some(day.as_str()) {
                self.messages.append(&message::day_separator(&day));
                last_day = Some(day);
                last_author = None;
            }

            if post.is_system() {
                self.messages.append(&message::build(
                    post,
                    state,
                    avatars,
                    actions,
                    RowOptions {
                        grouped: false,
                        show_thread_footer: false,
                    },
                ));
                last_author = None;
                continue;
            }

            let author = state.borrow().author_name(post);
            let grouped = last_author.as_deref() == Some(author.as_str())
                && post.create_at - last_at < message::GROUPING_WINDOW_MS;

            self.messages.append(&message::build(
                post,
                state,
                avatars,
                actions,
                RowOptions {
                    grouped,
                    show_thread_footer: true,
                },
            ));
            last_author = Some(author);
            last_at = post.create_at;
        }

        // Scrolling to the bottom only makes sense when the bottom is the
        // newest message; in a history block it would jump into the past.
        if at_latest && *self.pinned_to_bottom.borrow() {
            let adjustment = self.scroller.vadjustment();
            // The new rows are not allocated yet, so defer a frame.
            glib::idle_add_local_once(move || {
                adjustment.set_value(adjustment.upper() - adjustment.page_size());
            });
        }
    }
}

/// Colours a header button while its feature is on.
///
/// `.flat` paints the background transparent and comes later in the Adwaita
/// stylesheet than `.suggested-action`, so a flat button given an accent class
/// stays stubbornly grey. Dropping `.flat` is what actually shows the state.
fn set_active(button: &gtk::Button, active: bool, class: &str) {
    if active {
        button.remove_css_class("flat");
        button.add_css_class(class);
    } else {
        button.remove_css_class(class);
        button.add_css_class("flat");
    }
}
