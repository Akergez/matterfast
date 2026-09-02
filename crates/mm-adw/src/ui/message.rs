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
    /// Everything behind the "…" menu: post id and what to do with it.
    pub post_action: Rc<dyn Fn(String, PostAction)>,
}

/// The overflow menu's entries. One enum rather than one callback each: they
/// all travel the same path to the action loop, and the row does not care what
/// any of them mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostAction {
    Edit,
    Delete,
    Pin,
    Unpin,
    Save,
    Unsave,
    MarkUnread,
    CopyLink,
    CopyText,
    /// Ask the LLM agent to summarise this thread.
    Summarise,
    /// Have the server DM you about this later.
    Remind,
    /// Show what this message said before it was edited.
    History,
    /// Move this thread to another channel.
    MoveThread,
    /// Confirm you have read a priority message, or take it back.
    Acknowledge,
    Unacknowledge,
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
    let presence = st.presence(&author_id);
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

    for block in crate::markdown::parse(&post.message) {
        body.append(&render_block(block));
    }

    if post.is_edited() {
        let edited = gtk::Label::builder().label("(edited)").xalign(0.0).build();
        edited.add_css_class("message-timestamp");
        body.append(&edited);
    }

    for file in post.files() {
        body.append(&attachment(file, avatars, state));
    }

    // A link to another message renders as that message. The server resolves
    // it for us into the embed, so this is presentation only — following the
    // link by hand would be a second fetch for something already here.
    for embed in post.embeds() {
        if let Some(preview) = permalink_preview(embed, state, avatars, actions) {
            body.append(&preview);
        }
    }

    if post
        .priority()
        .and_then(|p| p.requested_ack)
        .unwrap_or(false)
    {
        body.append(&acknowledgement(post, state, actions));
    }

    if let Some(strip) = reaction_strip(post, state, avatars, actions) {
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
    let mine = post.user_id == state.borrow().me.id;
    let saved = state.borrow().saved_posts.contains(&post.id);
    row.append(&hover_actions(
        post,
        actions,
        options.show_thread_footer,
        mine,
        saved,
    ));

    // An unconfirmed send stays dimmed until the server echoes it back.
    if post.is_pending() {
        row.add_css_class("message-pending");
    }

    row.upcast()
}

/// The small react / reply buttons on the right of a row.
/// The "please confirm you have read this" row on a priority message. Shown
/// as a button until you press it, then as who has.
fn acknowledgement(post: &Post, state: &SharedState, actions: &MessageActions) -> gtk::Widget {
    let st = state.borrow();
    let acks = post
        .metadata
        .as_ref()
        .map(|m| m.acknowledgements.as_slice())
        .unwrap_or_default();
    let mine = acks.iter().any(|a| a.user_id == st.me.id);

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(4)
        .build();

    let button = gtk::Button::builder()
        .label(if mine { "Acknowledged" } else { "Acknowledge" })
        .build();
    button.add_css_class("pill");
    if mine {
        button.add_css_class("success");
    } else {
        button.add_css_class("suggested-action");
    }
    button.connect_clicked({
        let actions = actions.clone();
        let post_id = post.id.clone();
        move |_| {
            (actions.post_action)(
                post_id.clone(),
                if mine {
                    PostAction::Unacknowledge
                } else {
                    PostAction::Acknowledge
                },
            )
        }
    });
    row.append(&button);

    if !acks.is_empty() {
        let names: Vec<String> = acks
            .iter()
            .filter_map(|a| st.users.get(&a.user_id))
            .map(|u| st.display_name(u))
            .collect();
        let count = gtk::Label::new(Some(&format!("{} acknowledged", acks.len())));
        count.add_css_class("dim-label");
        count.set_tooltip_text(Some(&names.join(", ")));
        row.append(&count);
    }

    row.upcast()
}

/// One emoji, however it has to be drawn: a Unicode glyph in a label, or a
/// custom upload as a small picture. A custom one that has not arrived shows
/// its shortcode, which is at least readable.
fn emoji_widget(name: &str, avatars: &Avatars) -> gtk::Widget {
    match emoji::resolve(name) {
        emoji::Rendered::Unicode(glyph) => gtk::Label::new(Some(glyph)).upcast(),
        emoji::Rendered::Custom => match avatars.custom_emoji(name) {
            Some(texture) => {
                let picture = gtk::Picture::for_paintable(&texture);
                picture.set_content_fit(gtk::ContentFit::Contain);
                picture.set_width_request(18);
                picture.set_height_request(18);
                picture.set_tooltip_text(Some(&format!(":{name}:")));
                picture.upcast()
            }
            None => gtk::Label::new(Some(&format!(":{name}:"))).upcast(),
        },
    }
}

/// The quoted message behind a permalink, as a compact card.
fn permalink_preview(
    embed: &mattermost_api::models::PostEmbed,
    state: &SharedState,
    avatars: &Avatars,
    actions: &MessageActions,
) -> Option<gtk::Widget> {
    if embed.r#type != "permalink" {
        return None;
    }
    // The embed carries the post under a `post` key; anything else is a
    // permalink the server could not resolve, and a card saying nothing is
    // worse than the bare link already in the text.
    let quoted: Post = serde_json::from_value(
        embed
            .data
            .as_ref()?
            .get("post")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    )
    .ok()?;

    let st = state.borrow();
    let author = st.author_name(&quoted);
    let channel = st
        .channel(&quoted.channel_id)
        .map(|c| st.channel_title(c))
        .unwrap_or_default();
    drop(st);

    let avatar = adw::Avatar::builder().size(20).build();
    avatars.apply(&avatar, &quoted.user_id, &author);

    let who = gtk::Label::new(Some(&author));
    who.add_css_class("message-author");
    let when = gtk::Label::new(Some(&format_time(quoted.create_at)));
    when.add_css_class("message-timestamp");

    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    header.append(&avatar);
    header.append(&who);
    header.append(&when);
    if !channel.is_empty() {
        let where_ = gtk::Label::new(Some(&format!("in {channel}")));
        where_.add_css_class("dim-label");
        where_.add_css_class("message-timestamp");
        header.append(&where_);
    }

    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();
    card.add_css_class("permalink-card");
    card.append(&header);
    for block in crate::markdown::parse(&quoted.message) {
        card.append(&render_block(block));
    }

    // Clicking it goes there, which is what the link would have done.
    let open = gtk::Button::builder().child(&card).build();
    open.add_css_class("flat");
    open.add_css_class("permalink-button");
    open.connect_clicked({
        let actions = actions.clone();
        let root = quoted.thread_root().to_string();
        move |_| (actions.open_thread)(root.clone())
    });
    Some(open.upcast())
}

/// An attached file. Images show themselves; everything else is a name and a
/// size, which is all there is to say about it without opening it.
fn attachment(
    file: &mattermost_api::models::FileInfo,
    avatars: &Avatars,
    state: &SharedState,
) -> gtk::Widget {
    if file.is_image() {
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::ScaleDown)
            .halign(gtk::Align::Start)
            .can_shrink(true)
            // Tall enough to recognise, short enough that an image does not
            // push the rest of the conversation off the screen.
            .height_request(180)
            .tooltip_text(&file.name)
            .build();
        picture.add_css_class("attachment-image");
        if let Some(texture) = avatars.file_thumbnail(&file.id) {
            picture.set_paintable(Some(&texture));
        }

        let open = gtk::Button::builder().child(&picture).build();
        open.add_css_class("flat");
        open.add_css_class("attachment-button");
        open.connect_clicked({
            let state = state.clone();
            let file = file.clone();
            move |button| open_image(&state, &file, button)
        });
        return open.upcast();
    }

    // Everything that is not an image: a name, a size, and a way to get it.
    let label = gtk::Label::builder()
        .label(format!("{}  ·  {}", file.name, file.human_size()))
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .build();

    let save = gtk::Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Save")
        .valign(gtk::Align::Center)
        .build();
    save.add_css_class("flat");
    save.add_css_class("circular");
    save.connect_clicked({
        let state = state.clone();
        let file = file.clone();
        move |button| save_attachment(&state, &file, button)
    });

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    row.add_css_class("dim-label");
    row.append(&gtk::Image::from_icon_name("mail-attachment-symbolic"));
    row.append(&label);
    row.append(&save);
    row.upcast()
}

/// Downloads an attachment to wherever the person says.
///
/// The file is behind the session token, so it is fetched and written here
/// rather than handed to anything else as a URL.
fn save_attachment(
    state: &SharedState,
    file: &mattermost_api::models::FileInfo,
    anchor: &gtk::Button,
) {
    let dialog = gtk::FileDialog::builder()
        .title("Save attachment")
        .initial_name(&file.name)
        .build();
    let parent = anchor.root().and_downcast::<gtk::Window>();
    let client = state.borrow().client.clone();
    let file_id = file.id.clone();

    dialog.save(
        parent.as_ref(),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let Ok(target) = result else { return };
            let Some(path) = target.path() else { return };
            let client = client.clone();
            let file_id = file_id.clone();
            crate::runtime::spawn(
                async move {
                    let bytes = client
                        .download_file(&file_id)
                        .await
                        .map_err(|e| e.to_string())?;
                    tokio::fs::write(&path, bytes)
                        .await
                        .map_err(|e| e.to_string())
                },
                |result| {
                    if let Err(e) = result {
                        tracing::warn!(error = %e, "could not save the attachment");
                    }
                },
            );
        },
    );
}

/// Opens the full-size image in its own window. The original is behind the
/// session token, so it is fetched rather than handed to an external viewer.
fn open_image(state: &SharedState, file: &mattermost_api::models::FileInfo, anchor: &gtk::Button) {
    let client = state.borrow().client.clone();
    let file_id = file.id.clone();
    let title = file.name.clone();
    let parent = anchor.root().and_downcast::<gtk::Window>();

    crate::runtime::spawn(
        async move { client.download_file(&file_id).await },
        move |result| {
            let Ok(bytes) = result else { return };
            let Ok(texture) = gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(bytes))
            else {
                return;
            };
            let picture = gtk::Picture::for_paintable(&texture);
            picture.set_content_fit(gtk::ContentFit::ScaleDown);

            let window = adw::Window::builder()
                .title(&title)
                .default_width(900)
                .default_height(640)
                .modal(false)
                .build();
            window.set_transient_for(parent.as_ref());
            let view = adw::ToolbarView::new();
            view.add_top_bar(&adw::HeaderBar::new());
            view.set_content(Some(&picture));
            window.set_content(Some(&view));
            window.present();
        },
    );
}

/// One piece of a message. Prose is a label with Pango markup; a code block is
/// a monospaced label that must *not* be told to read markup — its text is
/// literal, and code is exactly the content most likely to contain angle
/// brackets.
fn render_block(block: crate::markdown::Block) -> gtk::Widget {
    let label = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        // Selectable labels select all their text when they take keyboard
        // focus, so the first message would come up highlighted.
        .can_focus(false)
        .build();

    match block {
        crate::markdown::Block::Text(markup) => {
            label.set_markup(&markup);
            label.add_css_class("message-body");
            label.upcast()
        }
        crate::markdown::Block::Code { text, .. } => {
            label.set_text(&text);
            label.set_wrap(false);
            label.add_css_class("message-code");
            // Long lines scroll rather than widening the whole conversation.
            let scroller = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Automatic)
                .vscrollbar_policy(gtk::PolicyType::Never)
                .propagate_natural_height(true)
                .propagate_natural_width(true)
                .child(&label)
                .build();
            scroller.add_css_class("message-code-frame");
            scroller.upcast()
        }
    }
}

fn hover_actions(
    post: &Post,
    actions: &MessageActions,
    allow_thread: bool,
    mine: bool,
    saved: bool,
) -> gtk::Widget {
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

    // Saving is one click in every other client, so it is a button here too
    // rather than being buried in the menu.
    let save = gtk::Button::builder()
        .icon_name(if saved {
            "starred-symbolic"
        } else {
            "non-starred-symbolic"
        })
        .tooltip_text(if saved { "Remove from saved" } else { "Save" })
        .build();
    save.add_css_class("flat");
    save.add_css_class("circular");
    save.connect_clicked({
        let actions = actions.clone();
        let post_id = post.id.clone();
        move |_| {
            (actions.post_action)(
                post_id.clone(),
                if saved {
                    PostAction::Unsave
                } else {
                    PostAction::Save
                },
            )
        }
    });
    bar.append(&save);

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

    bar.append(&overflow_menu(post, actions, mine));
    bar.upcast()
}

/// The "…" menu. Editing and deleting are only offered on your own posts —
/// the server would refuse anyway, and an option that always fails is worse
/// than no option.
fn overflow_menu(post: &Post, actions: &MessageActions, mine: bool) -> gtk::Widget {
    let menu = gtk::gio::Menu::new();
    let group = gtk::gio::SimpleActionGroup::new();

    let mut entries: Vec<(&str, &str, PostAction)> = vec![
        ("Copy text", "copy-text", PostAction::CopyText),
        ("Copy link", "copy-link", PostAction::CopyLink),
        ("Mark as unread", "mark-unread", PostAction::MarkUnread),
        ("Remind me about this…", "remind", PostAction::Remind),
    ];
    if post.is_pinned {
        entries.push(("Unpin from channel", "unpin", PostAction::Unpin));
    } else {
        entries.push(("Pin to channel", "pin", PostAction::Pin));
    }
    // Only worth offering where there is a history to see.
    if post.is_edited() {
        entries.push(("Edit history", "history", PostAction::History));
    }
    if post.reply_count > 0 {
        entries.push(("Summarise thread", "summarise", PostAction::Summarise));
        entries.push(("Move thread…", "move", PostAction::MoveThread));
    }
    if mine {
        entries.push(("Edit", "edit", PostAction::Edit));
        entries.push(("Delete", "delete", PostAction::Delete));
    }

    for (label, name, action) in entries {
        let item = gtk::gio::SimpleAction::new(name, None);
        item.connect_activate({
            let actions = actions.clone();
            let post_id = post.id.clone();
            move |_, _| (actions.post_action)(post_id.clone(), action)
        });
        group.add_action(&item);
        menu.append(Some(label), Some(&format!("post.{name}")));
    }

    let button = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("More actions")
        .menu_model(&menu)
        .build();
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.insert_action_group("post", Some(&group));
    button.upcast()
}

fn reaction_picker(post: &Post, actions: &MessageActions) -> gtk::Popover {
    let quick = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(2)
        .build();

    // The eight most-used sit on top, because most reactions are one of them
    // and scrolling past them to find one would be the common case made slow.
    let all = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(8)
        .max_children_per_line(8)
        .homogeneous(true)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(220)
        .max_content_height(220)
        .width_request(280)
        .child(&all)
        .build();

    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search emoji")
        .build();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .build();
    content.append(&quick);
    content.append(&search);
    content.append(&scroller);

    let popover = gtk::Popover::builder().child(&content).build();

    // Filling the whole table costs a few thousand buttons, so the list is
    // rebuilt per search instead — and starts on a page of common ones.
    let fill = {
        let all = all.clone();
        let actions = actions.clone();
        let post_id = post.id.clone();
        let popover = popover.clone();
        move |term: &str| {
            while let Some(child) = all.first_child() {
                all.remove(&child);
            }
            let term = term.trim().to_lowercase();
            let matches = emojis::iter().filter_map(|e| {
                let name = e.shortcode()?;
                (term.is_empty() || name.contains(&term)).then_some((name, e.as_str()))
            });
            for (name, glyph) in matches.take(120) {
                let button = gtk::Button::builder()
                    .label(glyph)
                    .tooltip_text(format!(":{name}:"))
                    .build();
                button.add_css_class("flat");
                button.add_css_class("emoji-button");
                button.connect_clicked({
                    let actions = actions.clone();
                    let post_id = post_id.clone();
                    let name = name.to_string();
                    let popover = popover.clone();
                    move |_| {
                        popover.popdown();
                        (actions.toggle_reaction)(post_id.clone(), name.clone());
                    }
                });
                all.append(&button);
            }
        }
    };
    fill("");
    search.connect_search_changed({
        let fill = fill.clone();
        move |entry| fill(&entry.text())
    });

    let grid = quick;

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
    avatars: &Avatars,
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
        content.append(&emoji_widget(&name, avatars));
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
