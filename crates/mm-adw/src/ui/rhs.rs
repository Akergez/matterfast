//! The right-hand panel: a thread, or the notification inbox.
//!
//! Mattermost puts both in the same place and swaps between them, which is
//! worth copying — the two are alternatives, never side by side, and sharing
//! one surface keeps the window from growing a fifth column.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use mattermost_api::models::{Millis, Post};

use crate::avatars::Avatars;
use crate::state::{ChannelFeed, SharedState};
use crate::ui::message::{self, MessageActions, RowOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelMode {
    Hidden,
    /// Viewing the thread rooted at this post id.
    Thread(String),
    /// Mentions and unread threads.
    Inbox,
    /// Results for a search, held so a redraw does not lose them.
    Search(String),
}

pub struct RightPanel {
    pub widget: gtk::Box,
    mode: Rc<RefCell<PanelMode>>,
    title: gtk::Label,
    subtitle: gtk::Label,
    stack: gtk::Stack,

    thread_list: gtk::Box,
    thread_scroller: gtk::ScrolledWindow,
    thread_entry: gtk::TextView,
    /// Set while a draft is being restored, so it is not mistaken for typing.
    restoring: Rc<RefCell<bool>>,

    search_list: gtk::Box,

    inbox_stack: gtk::Stack,
    saved_list: gtk::Box,
    mentions_list: gtk::Box,
    threads_list: gtk::Box,

    on_open_thread: Rc<dyn Fn(String)>,
    /// (channel id, root post id — empty when the target is a root post)
    on_open_post: Rc<dyn Fn(String, String)>,
}

impl RightPanel {
    pub fn new(
        on_close: impl Fn() + 'static,
        on_reply: impl Fn(String) + 'static,
        on_open_thread: impl Fn(String) + 'static,
        on_open_post: impl Fn(String, String) + 'static,
        on_draft: impl Fn() + 'static,
    ) -> Rc<Self> {
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

        let close = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Close panel")
            .build();
        close.add_css_class("flat");
        close.connect_clicked(move |_| on_close());

        let header = adw::HeaderBar::builder()
            .title_widget(&title_box)
            .show_start_title_buttons(false)
            .build();
        header.pack_end(&close);

        // ---- thread page
        let thread_list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .valign(gtk::Align::End)
            .vexpand(true)
            .build();
        let thread_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&thread_list)
            .build();

        let thread_entry = gtk::TextView::builder()
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
            .max_content_height(140)
            .propagate_natural_height(true)
            .child(&thread_entry)
            .build();
        entry_frame.add_css_class("card");

        let send = gtk::Button::builder()
            .icon_name("document-send-symbolic")
            .tooltip_text("Reply  (Enter)")
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

        let on_reply = Rc::new(on_reply);
        let submit = {
            let entry = thread_entry.clone();
            let on_reply = on_reply.clone();
            move || {
                let buffer = entry.buffer();
                let (start, end) = buffer.bounds();
                let text = buffer.text(&start, &end, false).trim().to_string();
                if text.is_empty() {
                    return;
                }
                buffer.set_text("");
                on_reply(text);
            }
        };
        send.connect_clicked({
            let submit = submit.clone();
            move |_| submit()
        });
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
        thread_entry.add_controller(keys);

        let restoring = Rc::new(RefCell::new(false));
        thread_entry.buffer().connect_changed({
            let restoring = restoring.clone();
            let on_draft = Rc::new(on_draft);
            move |_| {
                if !*restoring.borrow() {
                    on_draft();
                }
            }
        });

        let thread_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        thread_page.append(&thread_scroller);
        thread_page.append(&composer);

        // ---- inbox page
        let mentions_list = list_box();
        let threads_list = list_box();

        let inbox_stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        let saved_list = list_box();
        inbox_stack.add_titled(&scroller(&mentions_list), Some("mentions"), "Mentions");
        inbox_stack.add_titled(&scroller(&threads_list), Some("threads"), "Threads");
        inbox_stack.add_titled(&scroller(&saved_list), Some("saved"), "Saved");

        let switcher = gtk::StackSwitcher::builder()
            .stack(&inbox_stack)
            .halign(gtk::Align::Center)
            .margin_top(8)
            .margin_bottom(4)
            .build();

        let inbox_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        inbox_page.append(&switcher);
        inbox_page.append(&inbox_stack);

        let stack = gtk::Stack::new();
        let search_list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        let search_page = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&search_list)
            .build();

        stack.add_named(&thread_page, Some("thread"));
        stack.add_named(&inbox_page, Some("inbox"));
        stack.add_named(&search_page, Some("search"));

        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        widget.add_css_class("right-panel");
        widget.append(&header);
        widget.append(&stack);

        Rc::new(RightPanel {
            widget,
            mode: Rc::new(RefCell::new(PanelMode::Hidden)),
            title,
            subtitle,
            stack,
            thread_list,
            thread_scroller,
            thread_entry,
            restoring,
            search_list,
            inbox_stack,
            saved_list,
            mentions_list,
            threads_list,
            on_open_thread: Rc::new(on_open_thread),
            on_open_post: Rc::new(on_open_post),
        })
    }

    pub fn mode(&self) -> PanelMode {
        self.mode.borrow().clone()
    }

    pub fn set_mode(&self, mode: PanelMode) {
        *self.mode.borrow_mut() = mode;
    }

    pub fn focus_composer(&self) {
        self.thread_entry.grab_focus();
    }

    /// What is in the thread's reply box.
    pub fn composer_text(&self) -> String {
        let buffer = self.thread_entry.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }

    /// Puts a thread draft back. Guarded like the channel composer's: setting
    /// the buffer fires `changed`, which must not be read as typing.
    pub fn set_composer_text(&self, text: &str) {
        if self.composer_text() == text {
            return;
        }
        *self.restoring.borrow_mut() = true;
        self.thread_entry.buffer().set_text(text);
        let buffer = self.thread_entry.buffer();
        buffer.place_cursor(&buffer.end_iter());
        *self.restoring.borrow_mut() = false;
    }

    /// Redraws whatever the panel is currently showing.
    pub fn refresh(&self, state: &SharedState, avatars: &Avatars, actions: &MessageActions) {
        match self.mode() {
            PanelMode::Hidden => {}
            PanelMode::Thread(root_id) => self.render_thread(&root_id, state, avatars, actions),
            PanelMode::Inbox => self.render_inbox(state, avatars),
            PanelMode::Search(terms) => self.render_search(&terms, state, avatars, actions),
        }
    }

    /// Search results, newest first, each one a jump into its channel.
    fn render_search(
        &self,
        terms: &str,
        state: &SharedState,
        avatars: &Avatars,
        actions: &MessageActions,
    ) {
        self.stack.set_visible_child_name("search");
        clear(&self.search_list);
        self.title.set_text("Search");
        self.subtitle.set_text(terms);
        self.subtitle.set_visible(true);

        let st = state.borrow();
        if st.searching {
            let spinner = gtk::Spinner::builder()
                .halign(gtk::Align::Center)
                .margin_top(24)
                .build();
            spinner.start();
            self.search_list.append(&spinner);
            return;
        }
        if st.search_results.is_empty() {
            self.search_list.append(
                &adw::StatusPage::builder()
                    .icon_name("system-search-symbolic")
                    .title("No matches")
                    .description("Nothing here matched that search.")
                    .css_classes(["compact"])
                    .build(),
            );
            return;
        }

        for post in &st.search_results {
            let channel = st
                .channel(&post.channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            let row = message::build(
                post,
                state,
                avatars,
                actions,
                RowOptions {
                    grouped: false,
                    show_thread_footer: false,
                },
            );
            // Which channel a hit came from is most of what makes it useful.
            let label = gtk::Label::builder()
                .label(&channel)
                .xalign(0.0)
                .margin_start(14)
                .margin_top(8)
                .build();
            label.add_css_class("caption-heading");
            label.add_css_class("dim-label");
            self.search_list.append(&label);
            self.search_list.append(&row);
        }
    }

    fn render_thread(
        &self,
        root_id: &str,
        state: &SharedState,
        avatars: &Avatars,
        actions: &MessageActions,
    ) {
        self.stack.set_visible_child_name("thread");
        clear(&self.thread_list);

        let st = state.borrow();
        let channel_title = st
            .threads
            .get(root_id)
            .and_then(|feed| feed.posts.first())
            .and_then(|p| st.channel(&p.channel_id))
            .map(|c| st.channel_title(c))
            .unwrap_or_default();
        drop(st);

        self.title.set_text("Thread");
        self.subtitle.set_text(&channel_title);
        self.subtitle.set_visible(!channel_title.is_empty());

        let st = state.borrow();
        // While the replies are in flight, show the root on its own if we
        // already hold it — the reader clicked a message they were looking at,
        // and an empty panel makes the click feel lost.
        let fallback;
        let feed = match st.threads.get(root_id) {
            Some(feed) => feed,
            None => match st.find_post(root_id) {
                Some(root) => {
                    fallback = ChannelFeed {
                        posts: vec![root.clone()],
                        ..Default::default()
                    };
                    &fallback
                }
                None => {
                    drop(st);
                    self.thread_list.append(
                        &adw::StatusPage::builder()
                            .title("Loading thread…")
                            .vexpand(true)
                            .build(),
                    );
                    return;
                }
            },
        };
        // The feed arrives sorted by `create_at` (see `ChannelFeed::from_list`,
        // which has to sort because the thread endpoint does not). That is
        // still not enough: two posts can share a millisecond on a busy server,
        // and then the conversation starts with a reply. So the root goes
        // first, always.
        let mut posts = feed.posts.clone();
        let root_index = posts.iter().position(|p| p.id == root_id);
        if let Some(index) = root_index {
            let root = posts.remove(index);
            posts.insert(0, root);
        }
        let reply_count = posts.len().saturating_sub(1) as i64;
        drop(st);

        let mut last_author: Option<String> = None;
        let mut last_at: Millis = 0;

        for (index, post) in posts.iter().enumerate() {
            if post.is_deleted() {
                continue;
            }
            // A divider after the root makes the thread readable at a glance,
            // and it is where Mattermost puts the reply count too.
            if index == 1 && reply_count > 0 {
                self.thread_list.append(&message::day_separator(&format!(
                    "{reply_count} {}",
                    message::plural(reply_count, "reply", "replies")
                )));
                last_author = None;
            }

            let author = state.borrow().author_name(post);
            let grouped = index != 0
                && last_author.as_deref() == Some(author.as_str())
                && post.create_at - last_at < message::GROUPING_WINDOW_MS;

            let row = message::build(
                post,
                state,
                avatars,
                actions,
                RowOptions {
                    grouped,
                    // We are already in the thread.
                    show_thread_footer: false,
                },
            );
            self.thread_list.append(&row);
            last_author = Some(author);
            last_at = post.create_at;
        }

        let adjustment = self.thread_scroller.vadjustment();
        glib::idle_add_local_once(move || {
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
        });
    }

    fn render_inbox(&self, state: &SharedState, avatars: &Avatars) {
        self.stack.set_visible_child_name("inbox");
        self.title.set_text("Inbox");
        self.subtitle.set_visible(false);
        clear(&self.mentions_list);
        clear(&self.threads_list);
        clear(&self.saved_list);

        let st = state.borrow();

        if st.mentions.is_empty() {
            self.mentions_list.append(&empty_state(
                "mail-unread-symbolic",
                "No recent mentions",
                "Messages that name you show up here.",
            ));
        }
        for post in &st.mentions {
            let channel = st
                .channel(&post.channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_else(|| "unknown channel".into());
            let row = inbox_row(
                avatars,
                &post.user_id,
                &st.author_name(post),
                &channel,
                &post.message,
                post.create_at,
                None,
            );
            let channel_id = post.channel_id.clone();
            let root_id = post.thread_root().to_string();
            let is_reply = post.is_reply();
            let open = self.on_open_post.clone();
            row.connect_clicked(move |_| {
                open(
                    channel_id.clone(),
                    if is_reply {
                        root_id.clone()
                    } else {
                        String::new()
                    },
                )
            });
            self.mentions_list.append(&row);
        }

        // Saved posts: the ones you flagged, newest first. They are held as a
        // set of ids, so this walks what is loaded rather than fetching —
        // anything not in memory shows up as soon as its channel is opened.
        let mut saved: Vec<&Post> = st
            .feeds
            .values()
            .chain(st.threads.values())
            .flat_map(|feed| feed.posts.iter())
            .filter(|p| st.saved_posts.contains(&p.id))
            .collect();
        saved.sort_by_key(|p| std::cmp::Reverse(p.create_at));
        saved.dedup_by_key(|p| p.id.clone());

        if saved.is_empty() {
            self.saved_list.append(&empty_state(
                "starred-symbolic",
                "Nothing saved",
                "Save a message from its menu and it waits here.",
            ));
        }
        for post in saved {
            let channel = st
                .channel(&post.channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_else(|| "unknown channel".into());
            let row = inbox_row(
                avatars,
                &post.user_id,
                &st.author_name(post),
                &channel,
                &post.message,
                post.create_at,
                None,
            );
            let channel_id = post.channel_id.clone();
            let root_id = post.thread_root().to_string();
            let is_reply = post.is_reply();
            let open = self.on_open_post.clone();
            row.connect_clicked(move |_| {
                open(
                    channel_id.clone(),
                    if is_reply {
                        root_id.clone()
                    } else {
                        String::new()
                    },
                )
            });
            self.saved_list.append(&row);
        }

        if st.thread_inbox.is_empty() {
            self.threads_list.append(&empty_state(
                "chat-message-new-symbolic",
                "No threads yet",
                "Threads you follow appear here.",
            ));
        }
        for thread in &st.thread_inbox {
            let author = st.author_name(&thread.post);
            let channel = st
                .channel(&thread.post.channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            let row = inbox_row(
                avatars,
                &thread.post.user_id,
                &author,
                &channel,
                &thread.post.message,
                thread.last_reply_at.max(thread.post.create_at),
                Some((
                    thread.reply_count,
                    thread.unread_replies,
                    thread.unread_mentions,
                )),
            );
            let root_id = thread.id.clone();
            let open = self.on_open_thread.clone();
            row.connect_clicked(move |_| open(root_id.clone()));
            self.threads_list.append(&row);
        }
    }

    /// Switches the inbox to its threads tab — used when a badge is clicked.
    pub fn show_threads_tab(&self) {
        self.inbox_stack.set_visible_child_name("threads");
    }
}

fn inbox_row(
    avatars: &Avatars,
    user_id: &str,
    author: &str,
    channel: &str,
    message: &str,
    at: Millis,
    counts: Option<(i64, i64, i64)>,
) -> gtk::Button {
    let avatar = adw::Avatar::builder()
        .size(32)
        .valign(gtk::Align::Start)
        .build();
    avatars.apply(&avatar, user_id, author);

    let heading = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    let who = gtk::Label::builder().label(author).xalign(0.0).build();
    who.add_css_class("message-author");
    heading.append(&who);

    let where_ = gtk::Label::builder()
        .label(channel)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    where_.add_css_class("dim-label");
    where_.add_css_class("caption");
    heading.append(&where_);

    let when = gtk::Label::new(Some(&message::format_relative(at)));
    when.add_css_class("message-timestamp");
    heading.append(&when);

    let preview = gtk::Label::builder()
        .label(message.lines().next().unwrap_or_default())
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(2)
        .wrap(true)
        .build();

    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();
    body.append(&heading);
    body.append(&preview);

    if let Some((replies, unread_replies, unread_mentions)) = counts {
        let footer = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_top(2)
            .build();
        let count = gtk::Label::new(Some(&format!(
            "{replies} {}",
            message::plural(replies, "reply", "replies")
        )));
        count.add_css_class("message-timestamp");
        footer.append(&count);

        if unread_mentions > 0 {
            let badge = gtk::Label::new(Some(&unread_mentions.to_string()));
            badge.add_css_class("mention-badge");
            footer.append(&badge);
        } else if unread_replies > 0 {
            let dot = gtk::Label::new(Some("●"));
            dot.add_css_class("accent");
            dot.add_css_class("presence-dot");
            dot.set_tooltip_text(Some("Unread replies"));
            footer.append(&dot);
        }
        body.append(&footer);
    }

    let row_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(4)
        .margin_end(4)
        .build();
    row_box.append(&avatar);
    row_box.append(&body);

    // A button rather than a list row: the whole entry is one target, and
    // wiring the click per row is simpler than fishing data back out of a
    // ListBox selection.
    let button = gtk::Button::builder().child(&row_box).build();
    button.add_css_class("flat");
    button.add_css_class("inbox-row");
    button
}

fn list_box() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build()
}

fn scroller(child: &gtk::Box) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(child)
        .build()
}

fn empty_state(icon: &str, title: &str, description: &str) -> gtk::Widget {
    adw::StatusPage::builder()
        .icon_name(icon)
        .title(title)
        .description(description)
        .vexpand(true)
        .build()
        .upcast()
}

fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
