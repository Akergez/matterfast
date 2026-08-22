//! One message row, shared by the channel feed and the thread panel.
//!
//! Both views draw the same thing with small differences (a thread never shows
//! a "N replies" footer, because you are already in the thread), so the
//! rendering lives here and the differences are parameters.

use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use mattermost_api::models::{Millis, Post};

use crate::avatars::Avatars;
use crate::emoji;
use crate::state::SharedState;

/// Messages from the same author within this window are drawn as one group,
/// without repeating the avatar and name.
pub const GROUPING_WINDOW_MS: Millis = 5 * 60 * 1000;

/// What a message row can ask the application to do.
#[derive(Clone)]
pub struct MessageActions {
    /// Open the thread rooted at this post id.
    pub open_thread: Rc<dyn Fn(String)>,
    /// Add or remove our own reaction: post id, emoji name.
    pub toggle_reaction: Rc<dyn Fn(String, String)>,
    /// Show the profile card for a user, anchored on the given widget.
    pub show_profile: Rc<dyn Fn(String, gtk::Widget)>,
}

pub struct RowOptions {
    /// Continuation of the previous author's group: no avatar, no name.
    pub grouped: bool,
    /// Offer the "N replies" footer. False inside a thread.
    pub show_thread_footer: bool,
}

/// Builds a message row.
pub fn build(
    post: &Post,
    state: &SharedState,
    avatars: &Avatars,
    actions: &MessageActions,
    options: RowOptions,
) -> gtk::Widget {
    let st = state.borrow();
    let author_name = st.author_name(post);
    let author_id = post.user_id.clone();
    let presence = st.statuses.get(&author_id).copied().unwrap_or_default();
    drop(st);

    if post.is_system() {
        return system_row(post);
    }

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .margin_top(if options.grouped { 0 } else { 8 })
        .build();
    // The hover-reveal CSS needs a class to hang off: a plain GtkBox's CSS node
    // is `box`, so a `row:hover` selector would never match and the react and
    // reply buttons would stay invisible forever.
    row.add_css_class("message-row");

    if options.grouped {
        // Keep the text aligned with the messages above it.
        row.append(&gtk::Box::builder().width_request(40).build());
    } else {
        let avatar = adw::Avatar::builder()
            .size(40)
            .valign(gtk::Align::Start)
            .build();
        avatars.apply(&avatar, &author_id, &author_name);
        let avatar = super::profile::with_presence(&avatar, presence);

        // The avatar is the profile affordance, as it is in every Mattermost
        // client — so it has to look and behave like a button.
        let button = gtk::Button::builder()
            .child(&avatar)
            .valign(gtk::Align::Start)
            .tooltip_text(format!("Profile: {author_name}"))
            .build();
        button.add_css_class("flat");
        button.add_css_class("avatar-button");
        button.connect_clicked({
            let actions = actions.clone();
            let author_id = author_id.clone();
            move |b| (actions.show_profile)(author_id.clone(), b.clone().upcast())
        });
        row.append(&button);
    }

    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .hexpand(true)
        .build();

    if !options.grouped {
        let name = gtk::Button::builder().label(&author_name).build();
        name.add_css_class("flat");
        name.add_css_class("author-button");
        name.connect_clicked({
            let actions = actions.clone();
            let author_id = author_id.clone();
            move |b| (actions.show_profile)(author_id.clone(), b.clone().upcast())
        });

        let time = gtk::Label::builder()
            .label(format_time(post.create_at))
            .xalign(0.0)
            .build();
        time.add_css_class("message-timestamp");

        let meta = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        meta.append(&name);
        meta.append(&time);

        if let Some(priority) = post.priority() {
            if priority.is_urgent() || priority.is_important() {
                let tag = gtk::Label::new(Some(if priority.is_urgent() {
                    "URGENT"
                } else {
                    "IMPORTANT"
                }));
                tag.add_css_class("mention-badge");
                if priority.is_urgent() {
                    tag.add_css_class("urgent");
                }
                meta.append(&tag);
            }
        }
        body.append(&meta);
    }

    let text = gtk::Label::builder()
        .label(&post.message)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        // Selectable labels select all their text when they take keyboard
        // focus, so the first message would come up highlighted.
        .can_focus(false)
        .build();
    text.add_css_class("message-body");
    body.append(&text);

    if post.is_edited() {
        let edited = gtk::Label::builder().label("(edited)").xalign(0.0).build();
        edited.add_css_class("message-timestamp");
        body.append(&edited);
    }

    for file in post.files() {
        let attachment = gtk::Label::builder()
            .label(format!("📎 {}  ·  {}", file.name, file.human_size()))
            .xalign(0.0)
            .build();
        attachment.add_css_class("dim-label");
        body.append(&attachment);
    }

    if let Some(strip) = reaction_strip(post, state, actions) {
        body.append(&strip);
    }

    // Under CRT a reply never appears in the channel feed, so the only way into
    // a thread is this footer — it has to be present whenever there are replies.
    if options.show_thread_footer && post.reply_count > 0 {
        let label = format!(
            "{} {}",
            post.reply_count,
            plural(post.reply_count, "reply", "replies")
        );
        let open = gtk::Button::builder()
            .label(&label)
            .halign(gtk::Align::Start)
            .build();
        open.add_css_class("flat");
        open.add_css_class("thread-link");
        open.connect_clicked({
            let actions = actions.clone();
            let root = post.thread_root().to_string();
            move |_| (actions.open_thread)(root.clone())
        });
        body.append(&open);
    }
    // A message with no replies gets no footer: starting a thread is the reply
    // button in the hover bar, and a permanent "Reply" under every message is
    // just noise.

    row.append(&body);
    row.append(&hover_actions(post, actions, options.show_thread_footer));

    // An unconfirmed send stays dimmed until the server echoes it back.
    if post.is_pending() {
        row.add_css_class("message-pending");
    }

    row.upcast()
}

/// The small react / reply buttons on the right of a row.
fn hover_actions(post: &Post, actions: &MessageActions, allow_thread: bool) -> gtk::Widget {
    let bar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(2)
        .valign(gtk::Align::Start)
        .build();
    bar.add_css_class("message-actions");

    let react = gtk::MenuButton::builder()
        .icon_name("face-smile-symbolic")
        .tooltip_text("Add reaction")
        .build();
    react.add_css_class("flat");
    react.add_css_class("circular");
    react.set_popover(Some(&reaction_picker(post, actions)));
    bar.append(&react);

    if allow_thread {
        let reply = gtk::Button::builder()
            .icon_name("mail-reply-sender-symbolic")
            .tooltip_text("Reply in thread")
            .build();
        reply.add_css_class("flat");
        reply.add_css_class("circular");
        reply.connect_clicked({
            let actions = actions.clone();
            let root = post.thread_root().to_string();
            move |_| (actions.open_thread)(root.clone())
        });
        bar.append(&reply);
    }

    bar.upcast()
}

fn reaction_picker(post: &Post, actions: &MessageActions) -> gtk::Popover {
    let grid = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(2)
        .build();
    let popover = gtk::Popover::builder().child(&grid).build();

    for name in emoji::QUICK_REACTIONS {
        let button = gtk::Button::builder()
            .label(emoji::label(name))
            .tooltip_text(format!(":{name}:"))
            .build();
        button.add_css_class("flat");
        button.add_css_class("emoji-button");
        button.connect_clicked({
            let actions = actions.clone();
            let post_id = post.id.clone();
            let name = (*name).to_string();
            let popover = popover.clone();
            move |_| {
                popover.popdown();
                (actions.toggle_reaction)(post_id.clone(), name.clone());
            }
        });
        grid.append(&button);
    }
    popover
}

/// Reactions, collapsed by emoji and rendered as actual emoji rather than
/// `:shortcodes:`. Clicking a chip toggles our own reaction, as everywhere else.
fn reaction_strip(
    post: &Post,
    state: &SharedState,
    actions: &MessageActions,
) -> Option<gtk::Widget> {
    let reactions = post.reactions();
    if reactions.is_empty() {
        return None;
    }
    let me = state.borrow().me.id.clone();

    // Preserve first-seen order rather than sorting: it matches what the other
    // clients show and keeps chips from jumping around as counts change.
    let mut counted: Vec<(String, usize, bool)> = Vec::new();
    for reaction in reactions {
        let mine = reaction.user_id == me;
        match counted
            .iter_mut()
            .find(|(n, _, _)| *n == reaction.emoji_name)
        {
            Some((_, count, is_mine)) => {
                *count += 1;
                *is_mine |= mine;
            }
            None => counted.push((reaction.emoji_name.clone(), 1, mine)),
        }
    }

    let strip = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .margin_top(3)
        .build();

    for (name, count, mine) in counted {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();
        content.append(&gtk::Label::new(Some(&emoji::label(&name))));
        let n = gtk::Label::new(Some(&count.to_string()));
        n.add_css_class("reaction-count");
        content.append(&n);

        let chip = gtk::Button::builder()
            .child(&content)
            .tooltip_text(format!(":{name}:"))
            .build();
        chip.add_css_class("reaction-chip");
        if mine {
            chip.add_css_class("mine");
        }
        chip.connect_clicked({
            let actions = actions.clone();
            let post_id = post.id.clone();
            let name = name.clone();
            move |_| (actions.toggle_reaction)(post_id.clone(), name.clone())
        });
        strip.append(&chip);
    }
    Some(strip.upcast())
}

fn system_row(post: &Post) -> gtk::Widget {
    let label = gtk::Label::builder()
        .label(&post.message)
        .xalign(0.0)
        .wrap(true)
        .margin_start(52)
        .build();
    label.add_css_class("message-system");
    label.add_css_class("dim-label");
    label.upcast()
}

pub fn day_separator(day: &str) -> gtk::Widget {
    let label = gtk::Label::builder().label(day).build();
    label.add_css_class("day-separator");
    label.add_css_class("dim-label");

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    row.append(&separator_line());
    row.append(&label);
    row.append(&separator_line());
    row.upcast()
}

fn separator_line() -> gtk::Separator {
    gtk::Separator::builder()
        .orientation(gtk::Orientation::Horizontal)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build()
}

pub fn plural(n: i64, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 {
        one
    } else {
        many
    }
}

/// Mattermost timestamps are Unix **milliseconds**.
pub fn format_time(millis: Millis) -> String {
    glib::DateTime::from_unix_local(millis / 1000)
        .and_then(|dt| dt.format("%H:%M"))
        .map(|s| s.to_string())
        .unwrap_or_default()
}

pub fn format_day(millis: Millis) -> String {
    let Ok(dt) = glib::DateTime::from_unix_local(millis / 1000) else {
        return String::new();
    };
    if let Ok(now) = glib::DateTime::now_local() {
        if dt.ymd() == now.ymd() {
            return "Today".to_string();
        }
        if let Ok(yesterday) = now.add_days(-1) {
            if dt.ymd() == yesterday.ymd() {
                return "Yesterday".to_string();
            }
        }
    }
    dt.format("%A, %e %B %Y")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Relative time for list views ("2m", "3h", "Tue").
pub fn format_relative(millis: Millis) -> String {
    let Ok(then) = glib::DateTime::from_unix_local(millis / 1000) else {
        return String::new();
    };
    let Ok(now) = glib::DateTime::now_local() else {
        return String::new();
    };
    let seconds = now.difference(&then).as_seconds();
    match seconds {
        s if s < 60 => "now".to_string(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s if s < 7 * 86_400 => then.format("%a").map(|s| s.to_string()).unwrap_or_default(),
        _ => then
            .format("%e %b")
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
    }
}
