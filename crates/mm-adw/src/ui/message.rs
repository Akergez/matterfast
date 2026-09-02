//! One message row, shared by the channel feed and the thread panel.
//!
//! Both views draw the same thing with small differences (a thread never shows
//! a "N replies" footer, because you are already in the thread), so the
//! rendering lives here and the differences are parameters.

use std::ops::Range;
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
    /// A link to another message on this server: go there rather than to a
    /// browser.
    pub open_permalink: Rc<dyn Fn(String)>,
    /// A clicked mention. By handle rather than by id, because that is all the
    /// text carries.
    pub show_profile_by_handle: Rc<dyn Fn(String, gtk::Widget)>,
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
    /// Send this message on to another channel.
    Forward,
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
    // Read while the borrow is open; the row is built after it closes.
    let custom_status = st.users.get(&author_id).and_then(|u| u.custom_status());
    drop(st);

    if post.is_system() {
        return system_row(post, state, actions);
    }

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .margin_top(if options.grouped { 0 } else { 8 })
        .build();
    // Tagged with its post id so the feed can be scrolled to a particular
    // message — that is how a search hit or an inbox entry lands on the thing
    // it named rather than merely in the right channel.
    unsafe { row.set_data("post-id", post.id.clone()) };
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

        // Somebody's custom status — the palm tree, the house — next to their
        // name, which is where it answers the question it exists to answer:
        // are they actually around.
        if let Some(chip) = custom_status.as_ref().and_then(custom_status_chip) {
            meta.append(&chip);
        }

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

    // A mention is only tinted when it names somebody we have actually seen —
    // usernames, and the special ones the server resolves for everyone.
    let known = {
        let state = state.clone();
        move |handle: &str| -> Option<String> {
            // The special ones address everybody and have no account behind
            // them, so they stay as written.
            if matches!(handle, "here" | "channel" | "all") {
                return Some(handle.to_string());
            }
            let st = state.borrow();
            let display = st.teammate_name_display().to_string();
            st.users
                .values()
                .find(|u| u.username == handle)
                .map(|u| u.display_name(&display))
                .filter(|name| !name.is_empty())
        }
    };
    for block in crate::markdown::parse_with(&post.message, &known) {
        body.append(&render_block(block, avatars, actions));
    }

    if post.is_edited() {
        let edited = gtk::Label::builder().label("(edited)").xalign(0.0).build();
        edited.add_css_class("message-timestamp");
        body.append(&edited);
    }

    // A webhook or plugin card. These usually come with an empty message, so
    // ignoring them renders nothing at all for the message.
    for card in post.attachments() {
        body.append(&attachment_card(&card, avatars, actions));
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
        // Who replied, before the count: a thread is worth opening because of
        // who is in it, and the faces answer that before the number does.
        let faces = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(2)
            .build();
        {
            let st = state.borrow();
            let display = st.teammate_name_display().to_string();
            // The post itself carries who replied — the server fills it in on
            // the way out under collapsed threads, so the faces are there
            // before the thread has been opened, let alone fetched. Falling
            // back to a loaded thread covers servers that leave it empty.
            let mut repliers: Vec<String> = post
                .participants
                .iter()
                .map(|p| p.id.clone())
                .filter(|id| !id.is_empty())
                .collect();
            if repliers.is_empty() {
                if let Some(thread) = st.threads.get(post.thread_root()) {
                    for reply in &thread.posts {
                        if reply.id != post.id && !repliers.contains(&reply.user_id) {
                            repliers.push(reply.user_id.clone());
                        }
                    }
                }
            }
            // The root's author is named above the message already.
            repliers.retain(|id| id != &post.user_id);

            for user_id in repliers.iter().take(THREAD_FACES) {
                let name = st
                    .users
                    .get(user_id)
                    .map(|u| u.display_name(&display))
                    .unwrap_or_default();
                let face = adw::Avatar::builder().size(20).build();
                avatars.apply(&face, user_id, &name);
                face.set_tooltip_text(Some(&name));
                faces.append(&face);
            }
        }

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

        let footer = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .halign(gtk::Align::Start)
            .build();
        if faces.first_child().is_some() {
            footer.append(&faces);
        }
        footer.append(&open);
        body.append(&footer);
    }
    // A message with no replies gets no footer: starting a thread is the reply
    // button in the hover bar, and a permanent "Reply" under every message is
    // just noise.

    row.append(&body);

    // Clicking the message opens its thread — the whole row, not just the
    // reply count, which is what every other client does and what people try
    // first. A secondary click is left alone so the context menu still works,
    // and text selection is unaffected because this only fires on a click that
    // did not become a drag.
    {
        let click = gtk::GestureClick::new();
        click.set_button(gtk::gdk::BUTTON_PRIMARY);
        // Bubble phase would never see the click at all on a message whose
        // text is selectable: the label claims the sequence for its own
        // selection handling and the gesture below it never fires. That is
        // why this worked only on the gaps between words.
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        click.connect_released({
            let actions = actions.clone();
            let root = post.thread_root().to_string();
            let allow_thread = options.show_thread_footer;
            move |gesture, presses, _, _| {
                // A double click is somebody selecting a word.
                if presses > 1 || !allow_thread {
                    return;
                }
                // Not while text is selected: the click that ends a selection
                // must not also navigate. Checked on the *next* idle rather
                // than now, because in the capture phase the label has not
                // updated its selection yet — asking here would always say
                // "nothing selected" and swallow every drag.
                let actions = actions.clone();
                let root = root.clone();
                let widget = gesture.widget();
                glib::idle_add_local_once(move || {
                    if widget.is_some_and(|w| has_selection(&w)) {
                        return;
                    }
                    (actions.open_thread)(root.clone());
                });
            }
        });
        row.add_controller(click);
    }

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
        // Start-aligned, or the button stretches to the width of the
        // conversation and reads as a banner rather than a thing to press.
        .halign(gtk::Align::Start)
        .build();

    let button = gtk::Button::builder()
        .label(if mine { "Acknowledged" } else { "Acknowledge" })
        // The row is start-aligned, but a button inside a horizontal box
        // still fills its allocation unless it says otherwise.
        .halign(gtk::Align::Start)
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

/// One rich card: a coloured stripe, a title that may be a link, some text,
/// and its fields laid out as label-and-value rows.
fn attachment_card(
    card: &mattermost_api::models::MessageAttachment,
    avatars: &Avatars,
    actions: &MessageActions,
) -> gtk::Widget {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();

    let line = |text: &str, classes: &[&str]| {
        let label = gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .can_focus(false)
            .build();
        for class in classes {
            label.add_css_class(class);
        }
        label
    };

    if !card.author_name.is_empty() {
        content.append(&line(&card.author_name, &["caption", "dim-label"]));
    }
    if !card.pretext.is_empty() {
        content.append(&line(&card.pretext, &["dim-label"]));
    }
    if !card.title.is_empty() {
        // A title with a link is a link; without one it is just bold.
        if card.title_link.is_empty() {
            content.append(&line(&card.title, &["heading"]));
        } else {
            let title = line("", &["heading"]);
            title.set_markup(&format!(
                "<a href=\"{}\">{}</a>",
                crate::markdown::escape_for_pango(&card.title_link),
                crate::markdown::escape_for_pango(&card.title)
            ));
            content.append(&title);
        }
    }
    if !card.text.is_empty() {
        for block in crate::markdown::parse(&card.text) {
            content.append(&render_block(block, avatars, actions));
        }
    }

    for field in &card.fields {
        let value = field.text();
        if field.title.is_empty() && value.is_empty() {
            continue;
        }
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .margin_top(4)
            .build();
        if !field.title.is_empty() {
            row.append(&line(&field.title, &["caption-heading"]));
        }
        if !value.is_empty() {
            row.append(&line(&value, &[]));
        }
        content.append(&row);
    }

    if !card.footer.is_empty() {
        content.append(&line(&card.footer, &["caption", "dim-label"]));
    }
    // Nothing usable in the card itself: the fallback is what it is for.
    if content.first_child().is_none() && !card.fallback.is_empty() {
        content.append(&line(&card.fallback, &[]));
    }

    content.set_hexpand(true);
    content.set_margin_start(10);

    // The sender's colour, where they gave one — it usually encodes the status
    // of whatever the card reports, so it carries meaning. Drawn rather than
    // styled: a per-widget stylesheet needs the style context, deprecated in
    // GTK 4.10, and one CSS provider per card would be a lot of stylesheets.
    let stripe = gtk::DrawingArea::builder().width_request(3).build();
    let colour = gtk::gdk::RGBA::parse(&card.color).ok();
    stripe.set_draw_func(move |area, cr, width, height| {
        let colour = colour.unwrap_or_else(|| {
            // No colour given: a muted version of whatever the text is, so it
            // still reads as a card in either light or dark.
            let fg = area.color();
            gtk::gdk::RGBA::new(fg.red(), fg.green(), fg.blue(), 0.3)
        });
        cr.set_source_rgba(
            colour.red() as f64,
            colour.green() as f64,
            colour.blue() as f64,
            colour.alpha() as f64,
        );
        cr.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = cr.fill();
    });

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .build();
    row.add_css_class("attachment-card");
    row.append(&stripe);
    row.append(&content);
    row.upcast()
}

/// A link the server resolved into a title and a description.
fn link_preview(embed: &mattermost_api::models::PostEmbed) -> Option<gtk::Widget> {
    let data = embed.data.as_ref()?;
    let read = |key: &str| {
        data.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let title = match read("title") {
        t if t.is_empty() => read("site_name"),
        t => t,
    };
    // Without a title there is nothing to preview that the link text does not
    // already say.
    if title.is_empty() {
        return None;
    }

    let heading = gtk::Label::builder()
        .label(&title)
        .xalign(0.0)
        .wrap(true)
        .build();
    heading.add_css_class("heading");

    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();
    card.add_css_class("permalink-card");
    card.append(&heading);

    let description = read("description");
    if !description.is_empty() {
        let body = gtk::Label::builder()
            .label(&description)
            .xalign(0.0)
            .wrap(true)
            .lines(3)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        body.add_css_class("dim-label");
        card.append(&body);
    }

    let open = gtk::Button::builder().child(&card).build();
    open.add_css_class("flat");
    open.add_css_class("permalink-button");
    let url = if embed.url.is_empty() {
        read("url")
    } else {
        embed.url.clone()
    };
    open.set_tooltip_text(Some(&url));
    open.connect_clicked(move |_| {
        let _ =
            gtk::gio::AppInfo::launch_default_for_uri(&url, None::<&gtk::gio::AppLaunchContext>);
    });
    Some(open.upcast())
}

/// One emoji, however it has to be drawn: a Unicode glyph in a label, or a
/// custom upload as a small picture. A custom one that has not arrived shows
/// its shortcode, which is at least readable.
fn emoji_widget(name: &str, avatars: &Avatars) -> gtk::Widget {
    match emoji::resolve(name) {
        emoji::Rendered::Unicode(glyph) => gtk::Label::new(Some(glyph)).upcast(),
        emoji::Rendered::Custom => match avatars.custom_emoji(name) {
            Some(texture) => custom_emoji_image(&texture, name).upcast(),
            None => gtk::Label::new(Some(&format!(":{name}:"))).upcast(),
        },
    }
}

/// A custom emoji's image at text size, for a chip or for the middle of a
/// sentence. A `GtkImage` rather than a `GtkPicture` because `pixel-size` is a
/// real size and not merely a minimum: the uploads are up to 128px square, and
/// a picture would ask for all of it.
fn custom_emoji_image(texture: &gtk::gdk::Texture, name: &str) -> gtk::Image {
    let image = gtk::Image::from_paintable(Some(texture));
    image.set_pixel_size(18);
    image.set_tooltip_text(Some(&format!(":{name}:")));
    image
}

/// The quoted message behind a permalink, as a compact card.
fn permalink_preview(
    embed: &mattermost_api::models::PostEmbed,
    state: &SharedState,
    avatars: &Avatars,
    actions: &MessageActions,
) -> Option<gtk::Widget> {
    if embed.r#type == "opengraph" || embed.r#type == "link" {
        return link_preview(embed);
    }
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
        card.append(&render_block(block, avatars, actions));
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
        // Big enough to actually look at. The other clients go to roughly
        // this, and a thumbnail you have to open to see is a thumbnail that
        // makes you open everything.
        //
        // Both bounds matter: capping height alone turns a wide screenshot
        // into a strip, and capping width alone lets a tall photo run down
        // the page.
        let (width, height) = scaled_size(file.width, file.height);
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .halign(gtk::Align::Start)
            .can_shrink(true)
            // Height only. A width *request* is a minimum, not a maximum, and
            // the thread panel is narrower than this — GTK then complains that
            // it cannot give the row the width it insists on. The clamp below
            // caps the width instead, which is what was actually meant.
            .height_request(height)
            .tooltip_text(&file.name)
            .build();
        picture.add_css_class("attachment-image");
        if let Some(texture) = avatars.file_thumbnail(&file.id) {
            picture.set_paintable(Some(&texture));
        }

        let sized = adw::Clamp::builder()
            .maximum_size(width)
            .halign(gtk::Align::Start)
            .child(&picture)
            .build();

        let open = gtk::Button::builder().child(&sized).build();
        open.add_css_class("flat");
        open.add_css_class("attachment-button");
        open.connect_clicked({
            let state = state.clone();
            let file = file.clone();
            move |button| open_image(&state, &file, button)
        });
        return open.upcast();
    }

    // Video and audio play in place. Nothing is fetched until the play button
    // is pressed — a channel with ten clips must not pull down ten clips.
    if super::media::is_playable(file) {
        let client = state.borrow().client.clone();
        // The player is kept alive by the closure the poster button holds, and
        // both die with the row. Its Drop removes the temp file it wrote.
        let player = Rc::new_cyclic(|weak: &std::rc::Weak<super::media::Player>| {
            let weak = weak.clone();
            // Video gets a still from the same thumbnail route images use.
            super::media::Player::new(file, avatars.file_thumbnail(&file.id), move |file_id| {
                let Some(player) = weak.upgrade() else { return };
                let client = client.clone();
                crate::runtime::spawn(
                    async move { client.download_file(&file_id).await },
                    move |result| match result {
                        Ok(bytes) => player.set_data(bytes),
                        Err(e) => tracing::warn!(error = %e, "could not fetch the media"),
                    },
                );
            })
        });
        let widget = player.widget.clone();
        // Tie the player's lifetime to the widget it drew.
        unsafe { widget.set_data("player", player) };
        return widget;
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
    let parent = anchor.root().and_downcast::<adw::ApplicationWindow>();
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
    let parent = anchor.root().and_downcast::<adw::ApplicationWindow>();

    crate::runtime::spawn(
        async move { client.download_file(&file_id).await },
        move |result| {
            let Ok(bytes) = result else { return };
            let Ok(texture) = gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(bytes))
            else {
                return;
            };
            let Some(parent) = parent else { return };
            // A lightbox rather than a second window: a picture is something
            // you glance at and dismiss, not something to manage in the window
            // list. Escape, a click outside it, or the close button put it
            // away, and it can be copied or saved from there.
            super::lightbox::show(&parent, &title, &texture);
        },
    );
}

/// One piece of a message. Prose is a label with Pango markup; a code block is
/// a monospaced label that must *not* be told to read markup — its text is
/// literal, and code is exactly the content most likely to contain angle
/// brackets.
///
/// The exception is prose with a custom emoji in it, which no label can draw:
/// that one block goes through [`rich_text`] instead.
fn render_block(
    block: crate::markdown::Block,
    avatars: &Avatars,
    actions: &MessageActions,
) -> gtk::Widget {
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
            // Asking for the picture also starts the fetch when it is missing,
            // and `Avatars::connect_loaded` redraws the conversation once it
            // lands — so an emoji that is not here yet simply stays a
            // `:shortcode:` in the label until the next pass.
            let emoji: Vec<(Range<usize>, gtk::gdk::Texture)> = custom_shortcodes(&markup)
                .into_iter()
                .filter_map(|(range, name)| Some((range, avatars.custom_emoji(name)?)))
                .collect();
            if !emoji.is_empty() {
                return rich_text(&markup, &emoji);
            }
            label.set_markup(&markup);
            label.add_css_class("message-body");
            // A link to a message on this server is not a web page: following
            // it in a browser would open the whole app again to show something
            // already on screen. Anything else goes to the browser as usual.
            label.connect_activate_link({
                let actions = actions.clone();
                move |label, url| follow_link(label, url, &actions)
            });
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

/// Marks put into the text so the buffer can be found again after the markup
/// has been parsed: where a picture goes, and where a link starts and ends.
/// Byte offsets into the markup do not survive parsing — entities collapse and
/// tags disappear — so the positions travel as characters instead.
///
/// All three are stripped out of the message first, so a mark in the buffer can
/// only ever be one this put there.
const EMOJI_MARK: char = '\u{FFFC}'; // OBJECT REPLACEMENT CHARACTER
const LINK_OPEN: char = '\u{FFF9}';
const LINK_CLOSE: char = '\u{FFFA}';

/// Prose with a custom emoji in it, as a `GtkTextView` with the pictures
/// anchored between the words.
///
/// A `GtkLabel` cannot hold a widget, so a block with a custom emoji has to be
/// drawn by something that can, and `GtkTextView` is the only such thing that
/// still wraps and still selects. It is *only* used for those blocks: the label
/// keeps link activation, keyboard-free selection and a single Pango layout for
/// free, and nearly every message is a block without a custom emoji in it, so
/// making all of them pay for this would be a lot of widget for a rare case.
///
/// What it costs, and what is paid back here: a text view has no idea what a
/// link is, so the `<a href>` the markup carries is turned into a styled span
/// plus a tag, and a click on that tag opens the URL.
fn rich_text(markup: &str, emoji: &[(Range<usize>, gtk::gdk::Texture)]) -> gtk::Widget {
    // 1. Rewrite the markup: pictures and link ends become marks, and the links
    //    keep their look through a span the text view *can* render.
    let mut out = String::with_capacity(markup.len());
    let mut links: Vec<String> = Vec::new();
    let mut i = 0;
    let mut next = 0;
    while let Some(rest) = markup.get(i..).filter(|rest| !rest.is_empty()) {
        if let Some((range, _)) = emoji.get(next).filter(|(r, _)| r.start == i) {
            out.push(EMOJI_MARK);
            i = range.end;
            next += 1;
        } else if let Some((url, _)) = rest
            .strip_prefix("<a href=\"")
            .and_then(|after| after.split_once("\">"))
        {
            i += "<a href=\"".len() + url.len() + "\">".len();
            links.push(unescape(url));
            out.push_str("<span foreground=\"#3584e4\" underline=\"single\">");
            out.push(LINK_OPEN);
        } else if rest.starts_with("</a>") {
            i += "</a>".len();
            out.push(LINK_CLOSE);
            out.push_str("</span>");
        } else {
            let ch = rest.chars().next().unwrap_or_default();
            i += ch.len_utf8();
            if !matches!(ch, EMOJI_MARK | LINK_OPEN | LINK_CLOSE) {
                out.push(ch);
            }
        }
    }

    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        // A selectable widget grabs the whole block when it takes focus, and a
        // conversation full of tab stops is not navigation anyone wants.
        .can_focus(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    view.add_css_class("message-body");
    // Transparent background and inherited colour: without it the block reads
    // as a pale slab of "entry" in the middle of the conversation.
    view.add_css_class("inline");

    let buffer = view.buffer();
    buffer.insert_markup(&mut buffer.start_iter(), &out);
    // Markup the parser rejects is inserted as nothing at all, and a message
    // that disappears is worse than one that lost its styling.
    if buffer.char_count() == 0 && !out.is_empty() {
        buffer.set_text(&out);
    }

    // 2. Swap each mark for its picture. Searching from the start every time is
    //    fine because the mark just consumed is gone by the next pass.
    for (range, texture) in emoji {
        let Some((mut start, mut end)) = buffer.start_iter().forward_search(
            &EMOJI_MARK.to_string(),
            gtk::TextSearchFlags::empty(),
            None,
        ) else {
            break;
        };
        buffer.delete(&mut start, &mut end);
        let anchor = buffer.create_child_anchor(&mut start);
        // Anchored children sit on the baseline, which is as close to the text
        // as a text view will place a widget.
        let name = markup[range.clone()].trim_matches(':');
        view.add_child_at_anchor(&custom_emoji_image(texture, name), &anchor);
    }

    // 3. And each link's marks for a tag spanning what was between them.
    let mut link_tags: Vec<(gtk::TextTag, String)> = Vec::new();
    for url in links {
        let find = |from: gtk::TextIter, mark: char| {
            from.forward_search(&mark.to_string(), gtk::TextSearchFlags::empty(), None)
        };
        let Some((mut open, mut open_end)) = find(buffer.start_iter(), LINK_OPEN) else {
            break;
        };
        buffer.delete(&mut open, &mut open_end);
        let start = open.offset();
        let Some((mut close, mut close_end)) = find(open, LINK_CLOSE) else {
            break;
        };
        buffer.delete(&mut close, &mut close_end);
        // Anonymous: the tag is only a way to ask "is this character a link?",
        // and two identical URLs in one message must not collide over a name.
        let Some(tag) = buffer.create_tag(None, &[]) else {
            break;
        };
        buffer.apply_tag(&tag, &buffer.iter_at_offset(start), &close);
        link_tags.push((tag, url));
    }

    if !link_tags.is_empty() {
        let click = gtk::GestureClick::new();
        // Ahead of the text view's own selection handling, which claims the
        // sequence on a drag and would otherwise swallow the release.
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        click.connect_released(move |gesture, presses, x, y| {
            let Some(view) = gesture.widget().and_downcast::<gtk::TextView>() else {
                return;
            };
            // A release that leaves text selected was a drag over the link, not
            // a press on it.
            if presses != 1 || view.buffer().has_selection() {
                return;
            }
            let (x, y) =
                view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            let Some(iter) = view.iter_at_location(x, y) else {
                return;
            };
            if let Some((_, url)) = link_tags.iter().find(|(tag, _)| iter.has_tag(tag)) {
                let _ = gtk::gio::AppInfo::launch_default_for_uri(
                    url,
                    None::<&gtk::gio::AppLaunchContext>,
                );
            }
        });
        view.add_controller(click);
    }

    view.upcast()
}

/// Every `:shortcode:` in a block of Pango markup that has no Unicode glyph —
/// the ones a label cannot draw — as a byte range and the name inside it.
///
/// The rules are `markdown::inline`'s, because it is that function's output
/// being read back: a name is alphanumeric with `_+-`, so `10:30` and `4:3` are
/// not shortcodes and an unclosed `:foo` is not one either. What is new is the
/// markup itself — a `href` full of colons, and inline code, which is someone
/// showing the shortcode rather than using it.
fn custom_shortcodes(markup: &str) -> Vec<(Range<usize>, &str)> {
    let bytes = markup.as_bytes();
    let mut found = Vec::new();
    let mut literal = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => {
                let tag = &markup[i + 1..];
                let end = tag.find('>').map_or(bytes.len(), |e| i + e + 2);
                if tag.starts_with("tt>") {
                    literal += 1;
                } else if tag.starts_with("/tt>") {
                    literal = literal.saturating_sub(1);
                }
                i = end;
            }
            b':' => {
                let rest = &markup[i + 1..];
                match rest
                    .find(':')
                    .filter(|end| is_shortcode_name(&rest[..*end]))
                {
                    Some(end) => {
                        let name = &rest[..end];
                        if literal == 0 && emoji::resolve(name) == emoji::Rendered::Custom {
                            found.push((i..i + end + 2, name));
                        }
                        i += end + 2;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    found
}

fn is_shortcode_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
}

/// Undoes Pango escaping, so the browser is handed the URL that was written.
/// `&amp;` last, or `&amp;lt;` would come back as `<`.
fn unescape(markup: &str) -> String {
    markup
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
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
        ("Forward…", "forward", PostAction::Forward),
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

/// A join, a leave, a rename.
///
/// The server writes the raw handle into the text — "sin joined the channel",
/// not the name anyone recognises — and names the people involved in `props`
/// under `username`, `addedUsername` and `removedUsername`. Those are what
/// gets swapped, rather than scanning the sentence for anything that looks
/// like a name: the wording is localised, so pattern-matching it would work in
/// English and nowhere else.
fn system_row(post: &Post, state: &SharedState, actions: &MessageActions) -> gtk::Widget {
    let text = {
        let st = state.borrow();
        let display = st.teammate_name_display().to_string();
        let mut text = post.message.clone();

        for (key, value) in &post.props {
            if !key.to_lowercase().ends_with("username") {
                continue;
            }
            let Some(handle) = value.as_str().filter(|h| !h.is_empty()) else {
                continue;
            };
            let Some(name) = st
                .users
                .values()
                .find(|u| u.username == handle)
                .map(|u| u.display_name(&display))
                .filter(|name| !name.is_empty() && name != handle)
            else {
                continue;
            };
            // The `@` form first, or replacing the bare handle would leave a
            // stray `@` in front of the display name.
            text = text.replace(&format!("@{handle}"), &name);
            text = replace_word(&text, handle, &name);
        }
        text
    };

    // Rendered rather than printed: the names in it are mentions, and they
    // should read like the ones in an ordinary message rather than as plain
    // grey text.
    let known = {
        let state = state.clone();
        move |handle: &str| -> Option<String> {
            let st = state.borrow();
            let display = st.teammate_name_display().to_string();
            st.users
                .values()
                .find(|u| u.username == handle)
                .map(|u| u.display_name(&display))
                .filter(|name| !name.is_empty())
        }
    };

    let label = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .margin_start(52)
        .build();
    match crate::markdown::parse_with(&text, &known).first() {
        Some(crate::markdown::Block::Text(markup)) => label.set_markup(markup),
        _ => label.set_text(&text),
    }
    label.connect_activate_link({
        let actions = actions.clone();
        move |label, url| follow_link(label, url, &actions)
    });
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
/// Replaces whole-word occurrences only, so a handle that happens to be a
/// substring of another word is left alone.
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(word) {
        let before_ok = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after = &rest[at + word.len()..];
        let after_ok = after
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');

        out.push_str(&rest[..at]);
        if before_ok && after_ok {
            out.push_str(with);
        } else {
            out.push_str(word);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Someone's custom status as a small chip: the emoji, with the text on
/// hover. The text goes in the tooltip rather than the row because a status
/// can be a sentence and the author line is not the place for one.
fn custom_status_chip(status: &mattermost_api::models::CustomStatus) -> Option<gtk::Widget> {
    if status.emoji.is_empty() && status.text.is_empty() {
        return None;
    }
    // Expired statuses stay on the user object until the server clears them,
    // so an old "on holiday" would otherwise sit next to someone's name for
    // weeks after they came back.
    if let Some(expiry) = status.expires_at.as_deref().filter(|e| !e.is_empty()) {
        if let Ok(when) = glib::DateTime::from_iso8601(expiry, None) {
            if when.to_unix() < glib::real_time() / 1_000_000 {
                return None;
            }
        }
    }

    let label = gtk::Label::new(Some(&crate::emoji::label(&status.emoji)));
    label.add_css_class("custom-status");
    label.set_tooltip_text(Some(&custom_status_tooltip(status)));
    Some(label.upcast())
}

/// The status as a person would read it: what it says, and until when.
///
/// The expiry is most of the information — "on holiday" matters differently
/// depending on whether they are back this afternoon or next week — and it is
/// what the other clients show alongside it.
pub fn custom_status_tooltip(status: &mattermost_api::models::CustomStatus) -> String {
    let text = if status.text.is_empty() {
        format!(":{}:", status.emoji)
    } else {
        status.text.clone()
    };
    match status.expires_at.as_deref().and_then(expiry_phrase) {
        Some(until) => format!("{text}\n{until}"),
        None => text,
    }
}

/// "Until 15:30" for today, "Until tomorrow at 15:30", "Until Friday at
/// 15:30" within the week, and a date beyond that — the same ladder the web
/// client walks, because "until 15:30" is useless if it means next Thursday.
fn expiry_phrase(expires_at: &str) -> Option<String> {
    if expires_at.is_empty() {
        return None;
    }
    let when = glib::DateTime::from_iso8601(expires_at, None).ok()?;
    let now = glib::DateTime::now_local().ok()?;
    // Already gone: the caller drops the whole chip in that case, but a stale
    // one must never claim a time in the past.
    if when.to_unix() <= now.to_unix() {
        return None;
    }

    let time = when.format("%H:%M").ok()?.to_string();
    let days = when.day_of_year() - now.day_of_year();
    // Across a year boundary the day numbers reset, so fall through to the
    // date rather than reporting a negative difference.
    let phrase = match days {
        0 if when.year() == now.year() => format!("Until {time}"),
        1 if when.year() == now.year() => format!("Until tomorrow at {time}"),
        2..=6 if when.year() == now.year() => {
            format!("Until {} at {time}", when.format("%A").ok()?)
        }
        _ => format!("Until {}", when.format("%e %B").ok()?.trim()),
    };
    Some(phrase)
}

/// Whether anything inside this row has selected text. A click that ends a
/// selection is a selection, not navigation.
fn has_selection(widget: &gtk::Widget) -> bool {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        let (start, end) = label.selection_bounds().unwrap_or((0, 0));
        if start != end {
            return true;
        }
    }
    if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
        if view.buffer().has_selection() {
            return true;
        }
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        if has_selection(&node) {
            return true;
        }
        child = node.next_sibling();
    }
    false
}

/// The size to draw an attached image at: its own proportions, fitted inside
/// a box big enough to see and small enough to scroll past.
///
/// A file with no dimensions — some servers omit them — gets the full box and
/// `Contain` sorts it out once the picture arrives.
fn scaled_size(width: i32, height: i32) -> (i32, i32) {
    const MAX_WIDTH: f64 = 420.0;
    const MAX_HEIGHT: f64 = 350.0;

    if width <= 0 || height <= 0 {
        return (MAX_WIDTH as i32, MAX_HEIGHT as i32);
    }
    let scale = (MAX_WIDTH / width as f64)
        .min(MAX_HEIGHT / height as f64)
        // Never enlarge: a 64px sticker blown up to 420 is a blurry sticker.
        .min(1.0);
    (
        ((width as f64 * scale).round() as i32).max(1),
        ((height as f64 * scale).round() as i32).max(1),
    )
}

/// Decides what a clicked link means: a person, a message on this server, or
/// an ordinary web page that the browser should have.
fn follow_link(label: &gtk::Label, url: &str, actions: &MessageActions) -> glib::Propagation {
    if let Some(handle) = url.strip_prefix(crate::markdown::MENTION_SCHEME) {
        (actions.show_profile_by_handle)(handle.to_string(), label.clone().upcast());
        return glib::Propagation::Stop;
    }
    if let Some(post_id) = permalink(url) {
        (actions.open_permalink)(post_id);
        return glib::Propagation::Stop;
    }
    glib::Propagation::Proceed
}

/// The post id in a Mattermost permalink, if that is what this is.
///
/// The shape is `<site>/<team>/pl/<post id>`, and the team part is sometimes
/// `_redirect` — which is why the match is on the `/pl/` segment rather than
/// on the whole URL.
fn permalink(url: &str) -> Option<String> {
    let (_, tail) = url.split_once("/pl/")?;
    let id = tail.split(['/', '?', '#']).next()?;
    // Mattermost ids are 26 characters of lowercase alphanumerics; anything
    // else is a different site that happens to have /pl/ in its path.
    (id.len() == 26 && id.chars().all(|c| c.is_ascii_alphanumeric())).then(|| id.to_string())
}

/// How many repliers to show before the count speaks for itself.
const THREAD_FACES: usize = 5;

/// The "new messages" landmark: a rule with a label, drawn where reading
/// stopped.
pub fn unread_line() -> gtk::Widget {
    let label = gtk::Label::new(Some("New messages"));
    label.add_css_class("unread-line-label");

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    row.add_css_class("unread-line");
    row.append(&rule());
    row.append(&label);
    row.append(&rule());
    row.upcast()
}

fn rule() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.set_valign(gtk::Align::Center);
    separator.set_hexpand(true);
    separator
}

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

#[cfg(test)]
mod size_tests {
    use super::scaled_size;

    #[test]
    fn fits_the_box_without_enlarging() {
        // A tall photo is bounded by height, a wide one by width.
        assert_eq!(scaled_size(3000, 4000), (263, 350));
        assert_eq!(scaled_size(4000, 1000), (420, 105));
        // Smaller than the box: left alone.
        assert_eq!(scaled_size(64, 64), (64, 64));
        // Unknown: the full box, and Contain sorts it out.
        assert_eq!(scaled_size(0, 0), (420, 350));
    }
}

#[cfg(test)]
mod permalink_tests {
    use super::permalink;

    #[test]
    fn recognises_a_message_link() {
        assert_eq!(
            permalink("https://mm.example.com/team/pl/gedfji9g1pbjjehsngn9j17fzr").as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );
        // The double slash a reminder message produces, and _redirect.
        assert_eq!(
            permalink("https://mm.example.com//pl/gedfji9g1pbjjehsngn9j17fzr").as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );
        assert_eq!(
            permalink("https://mm.example.com/_redirect/pl/gedfji9g1pbjjehsngn9j17fzr?x=1")
                .as_deref(),
            Some("gedfji9g1pbjjehsngn9j17fzr")
        );

        // Not ours.
        assert_eq!(permalink("https://example.com/pl/short"), None);
        assert_eq!(permalink("https://example.com/blog/post"), None);
    }
}

#[cfg(test)]
mod expiry_tests {
    use super::expiry_phrase;

    fn in_hours(hours: i32) -> String {
        gtk::glib::DateTime::now_local()
            .unwrap()
            .add_hours(hours)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .unwrap()
            .to_string()
    }

    #[test]
    fn says_when_it_clears() {
        // An hour out is a time; four days out has to name the day, or
        // "until 15:30" reads as this afternoon.
        assert!(expiry_phrase(&in_hours(1)).unwrap().starts_with("Until "));
        let far = expiry_phrase(&in_hours(24 * 4)).unwrap();
        assert!(far.contains(" at "), "got {far:?}");

        // Nothing to say, and nothing to claim.
        assert_eq!(expiry_phrase(""), None);
        assert_eq!(expiry_phrase("not a date"), None);
        // In the past: never phrased as though it were still coming.
        assert_eq!(expiry_phrase(&in_hours(-2)), None);
    }
}

#[cfg(test)]
mod word_tests {
    use super::replace_word;

    #[test]
    fn replaces_whole_words_only() {
        assert_eq!(replace_word("sin joined", "sin", "Anna"), "Anna joined");
        // A handle inside another word is not that person.
        assert_eq!(
            replace_word("sinner joined", "sin", "Anna"),
            "sinner joined"
        );
        assert_eq!(replace_word("ask sin.", "sin", "Anna"), "ask Anna.");
        assert_eq!(replace_word("sin_bot left", "sin", "Anna"), "sin_bot left");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(markup: &str) -> Vec<&str> {
        custom_shortcodes(markup)
            .into_iter()
            .map(|(_, name)| name)
            .collect()
    }

    #[test]
    fn a_custom_shortcode_is_found_where_it_stands() {
        let markup = "ship it <b>:shipit:</b> now";
        assert_eq!(names(markup), ["shipit"]);
        let (range, _) = custom_shortcodes(markup).remove(0);
        assert_eq!(&markup[range], ":shipit:");
    }

    #[test]
    fn colons_that_are_not_shortcodes_are_not_pictures() {
        assert!(names("at 10:30 sharp").is_empty());
        assert!(names("ratio 4:3").is_empty());
        // Already a glyph by the time this runs: Pango's job, not a picture.
        assert!(names("nice :tada:").is_empty());
        assert!(names("what :foo").is_empty());
    }

    #[test]
    fn markup_is_read_as_markup_and_not_as_prose() {
        // A URL is full of colons and none of them are emoji.
        assert!(names("<a href=\"https://x.test/a:shipit:b\">link</a>").is_empty());
        // Inline code is someone showing the shortcode, not using it.
        assert!(names("<tt><span background=\"#00000018\"> :shipit: </span></tt>").is_empty());
        // The visible text of a link still counts.
        assert_eq!(names("<a href=\"https://x.test\">:shipit:</a>"), ["shipit"]);
    }
}
