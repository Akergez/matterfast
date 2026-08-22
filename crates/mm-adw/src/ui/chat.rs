//! Pane 3: the conversation — header, call banner, message list, composer.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use mattermost_api::models::Millis;

use crate::avatars::Avatars;
use crate::state::SharedState;
use crate::ui::message::{self, MessageActions, RowOptions};

pub struct ChatView {
    pub widget: gtk::Box,
    title: gtk::Label,
    subtitle: gtk::Label,
    messages: gtk::Box,
    scroller: gtk::ScrolledWindow,
    entry: gtk::TextView,
    call_button: gtk::Button,
    mute_button: gtk::Button,
    record_button: gtk::Button,
    screen_button: gtk::Button,
    camera_button: gtk::Button,
    inbox_button: gtk::Button,
    inbox_badge: gtk::Label,
    call_banner: gtk::Box,
    call_banner_label: gtk::Label,
    join_button: gtk::Button,
    stack: gtk::Stack,
    /// False while the reader is scrolled up in history, so live messages do
    /// not yank them back to the bottom.
    pinned_to_bottom: Rc<RefCell<bool>>,
}

impl ChatView {
    pub fn new(
        on_send: impl Fn(String) + 'static,
        on_call: impl Fn() + 'static,
        on_mute: impl Fn() + 'static,
        on_record: impl Fn() + 'static,
        on_screen: impl Fn() + 'static,
        on_camera: impl Fn() + 'static,
        on_inbox: impl Fn() + 'static,
    ) -> Self {
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

        // Only meaningful inside a call, so it is not there outside one.
        let mute_button = gtk::Button::builder()
            .icon_name("microphone-disabled-symbolic")
            .tooltip_text("Unmute")
            .visible(false)
            .build();
        mute_button.add_css_class("flat");
        mute_button.connect_clicked(move |_| on_mute());

        let record_button = gtk::Button::builder()
            .icon_name("media-record-symbolic")
            .tooltip_text("Record the call")
            .visible(false)
            .build();
        record_button.add_css_class("flat");
        record_button.connect_clicked(move |_| on_record());

        let screen_button = gtk::Button::builder()
            .icon_name("video-display-symbolic")
            .tooltip_text("Share your screen")
            .visible(false)
            .build();
        screen_button.add_css_class("flat");
        screen_button.connect_clicked(move |_| on_screen());

        let camera_button = gtk::Button::builder()
            .icon_name("camera-web-symbolic")
            .tooltip_text("Turn the camera on")
            .visible(false)
            .build();
        camera_button.add_css_class("flat");
        camera_button.connect_clicked(move |_| on_camera());

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
        header.pack_end(&call_button);
        header.pack_end(&mute_button);
        header.pack_end(&record_button);
        header.pack_end(&screen_button);
        header.pack_end(&camera_button);
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

        let composer = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_top(6)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
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
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed({
            let submit = submit.clone();
            move |_, key, _, modifier| {
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

        let conversation = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        conversation.append(&call_banner);
        conversation.append(&scroller);
        conversation.append(&composer);

        let placeholder = adw::StatusPage::builder()
            .icon_name("chat-message-new-symbolic")
            .title("No channel selected")
            .description("Pick a channel from the sidebar to start reading.")
            .build();

        let stack = gtk::Stack::new();
        stack.add_named(&placeholder, Some("empty"));
        stack.add_named(&conversation, Some("conversation"));
        stack.set_visible_child_name("empty");

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&stack));

        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        widget.append(&toolbar);

        ChatView {
            widget,
            title,
            subtitle,
            messages,
            scroller,
            entry,
            call_button,
            mute_button,
            record_button,
            screen_button,
            camera_button,
            inbox_button,
            inbox_badge,
            call_banner,
            call_banner_label,
            join_button,
            stack,
            pinned_to_bottom,
        }
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

    /// Reflects our own membership: the call button turns into hang-up and the
    /// microphone toggle appears next to it.
    pub fn set_in_call(
        &self,
        in_call: bool,
        ongoing: bool,
        muted: bool,
        recording: bool,
        sharing: bool,
        on_camera: bool,
    ) {
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
        self.mute_button.set_visible(in_call);
        self.mute_button.set_icon_name(if muted {
            "microphone-disabled-symbolic"
        } else {
            "audio-input-microphone-symbolic"
        });
        self.mute_button
            .set_tooltip_text(Some(if muted { "Unmute" } else { "Mute" }));
        set_active(&self.mute_button, !muted, "suggested-action");
        self.join_button.set_visible(!in_call);

        self.record_button.set_visible(in_call);
        self.record_button.set_tooltip_text(Some(if recording {
            "Stop recording"
        } else {
            "Record the call"
        }));
        set_active(&self.record_button, recording, "destructive-action");

        for (button, on, off, active) in [
            (
                &self.screen_button,
                "Stop sharing your screen",
                "Share your screen",
                sharing,
            ),
            (
                &self.camera_button,
                "Turn the camera off",
                "Turn the camera on",
                on_camera,
            ),
        ] {
            button.set_visible(in_call);
            button.set_tooltip_text(Some(if active { on } else { off }));
            set_active(button, active, "suggested-action");
        }
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
        let inbox_count = st
            .mentions
            .len()
            .min(99)
            .max(st.unread_threads().max(0) as usize) as i64;
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

        self.stack.set_visible_child_name("conversation");
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
