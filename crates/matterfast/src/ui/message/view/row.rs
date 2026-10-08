use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, ElementId, FontWeight, SharedString};
use mattermost_api::models::Post;

use super::acknowledgement::acknowledgement;
use super::attachment::attachment;
use super::attachment_card::attachment_card;
use super::element_id::eid;
use super::embed_preview::embed_preview;
use super::emoji_element::emoji_element;
use super::hover_actions::hover_actions;
use super::in_view::in_view;
use super::markdown_view::markdown;
use super::reaction_strip::reaction_strip;
use super::thread_footer::thread_footer;
use crate::timefmt::format_time;
use crate::ui::message::rules::{custom_status_tooltip, status_is_live};
use crate::ui::message::RowOptions;
use crate::ui::{frame_log, kit, Ui};

/// Builds a message row.
///
/// `body` is the message as Markdown, prepared when the list was built rather
/// than here: this runs for every visible row on every frame.
pub fn row(
    ui: &Rc<Ui>,
    post: &Post,
    body: &SharedString,
    options: RowOptions,
    cx: &mut App,
) -> AnyElement {
    let st = ui.state.borrow();
    let author_name = st.author_name(post);
    let author_id = post.user_id.clone();
    let presence = st.presence(&author_id);
    let custom_status = st
        .users
        .get(&author_id)
        .and_then(|u| u.custom_status())
        .filter(status_is_live);
    let mine = post.user_id == st.me.id;
    let saved = st.saved_posts.contains(&post.id);

    let theme = cx.theme();
    let muted = theme.muted_foreground;

    let mut content = v_flex().flex_1().min_w_0().gap_0p5();

    if !options.grouped {
        let mut meta = h_flex().gap_1p5().items_center().child(
            div()
                .id("author")
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .hover(|style| style.underline())
                .child(author_name.clone())
                .on_click(ui.click({
                    let author_id = author_id.clone();
                    move |ui, cx| ui.show_profile(&author_id, cx)
                })),
        );

        // Somebody's custom status — the palm tree, the house — next to their
        // name, which is where it answers the question it exists to answer:
        // are they actually around.
        if let Some(status) = &custom_status {
            meta = meta.child(kit::with_tooltip(
                "status",
                emoji_element(ui, &status.emoji, 16.),
                custom_status_tooltip(status),
            ));
        }

        meta = meta.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format_time(post.create_at)),
        );

        if let Some(priority) = post.priority() {
            if priority.is_urgent() {
                meta = meta.child(kit::tag("URGENT", theme.danger, theme.danger_foreground));
            } else if priority.is_important() {
                meta = meta.child(kit::tag("IMPORTANT", theme.primary, theme.primary_foreground));
            }
        }
        content = content.child(frame_log::timed("meta", meta));
    }

    if !body.is_empty() {
        content = content.child(frame_log::timed(
            "body",
            markdown(eid("body"), body.clone()),
        ));
    }

    if post.is_edited() {
        content = content.child(div().text_xs().text_color(muted).child("(edited)"));
    }

    // A webhook or plugin card. These usually come with an empty message, so
    // ignoring them renders nothing at all for the message.
    for (index, card) in post.attachments().iter().enumerate() {
        content = content.child(frame_log::timed(
            "card",
            attachment_card(ui, &post.id, index, card, cx),
        ));
    }

    for (index, file) in post.files().iter().enumerate() {
        content = content.child(frame_log::timed("file", attachment(ui, index, file, cx)));
    }

    // A link to another message renders as that message. The server resolves
    // it for us into the embed, so this is presentation only — following the
    // link by hand would be a second fetch for something already here.
    for (index, embed) in post.embeds().iter().enumerate() {
        if let Some(preview) = embed_preview(ui, index, embed, &st, cx) {
            content = content.child(frame_log::timed("embed", preview));
        }
    }

    if post
        .priority()
        .and_then(|p| p.requested_ack)
        .unwrap_or(false)
    {
        content = content.child(acknowledgement(ui, post, &st, cx));
    }

    if let Some(strip) = reaction_strip(ui, post, &st, cx) {
        content = content.child(frame_log::timed("reactions", strip));
    }

    // Under CRT a reply never appears in the channel feed, so the only way into
    // a thread is this footer — it has to be present whenever there are replies.
    if options.show_thread_footer && post.reply_count > 0 {
        content = content.child(frame_log::timed(
            "footer",
            thread_footer(ui, post, &st, cx),
        ));
    }
    // A message with no replies gets no footer: starting a thread is the reply
    // button in the hover bar, and a permanent "Reply" under every message is
    // just noise.

    let theme = cx.theme();
    let leading: AnyElement = if options.grouped {
        // Keep the text aligned with the messages above it.
        div().w(px(40.)).flex_none().into_any_element()
    } else {
        // The avatar is the profile affordance, as it is in every Mattermost
        // client — so it has to behave like a button.
        div()
            .id("avatar")
            .flex_none()
            .cursor_pointer()
            .child(kit::avatar_with_presence(
                ui,
                &author_id,
                &author_name,
                40.,
                presence,
                cx,
            ))
            .on_click(ui.click({
                let author_id = author_id.clone();
                move |ui, cx| ui.show_profile(&author_id, cx)
            }))
            .into_any_element()
    };

    // Only the row under the pointer has the bar: see `Ui::hovered_post`.
    let hovered = ui.hovered_post.borrow().as_deref() == Some(post.id.as_str());
    let actions = hovered
        .then(|| hover_actions(ui, post, options.show_thread_footer, mine, saved, cx));
    drop(st);

    h_flex()
        .id(ElementId::Name(format!("post-{}", post.id).into()))
        .group("message")
        .relative()
        .w_full()
        .items_start()
        .gap_2p5()
        .px_3()
        .py_0p5()
        .when(!options.grouped, |row| row.mt_2())
        .rounded_md()
        .hover(|style| style.bg(theme.list_hover))
        .when(options.highlight, |row| row.bg(theme.accent))
        // An unconfirmed send stays dimmed until the server echoes it back.
        .when(post.is_pending(), |row| row.opacity(0.55))
        .child(frame_log::timed("avatar", leading))
        .child(content)
        // The toolkit reports this for a row that scrolls under a pointer
        // that is not moving, too.
        .on_hover({
            let ui = ui.clone();
            let post_id = post.id.clone();
            move |hovered, _, cx| {
                if !*hovered || ui.hovered_post.borrow().as_deref() == Some(post_id.as_str()) {
                    return;
                }
                ui.hovered_post.replace(Some(post_id.clone()));
                crate::ui::redraw(&[crate::ui::Part::Chat, crate::ui::Part::Right], cx);
            }
        })
        .when_some(actions, |row, actions| {
            row.child(
                div()
                    .absolute()
                    .top(px(-10.))
                    .right_3()
                    .invisible()
                    .group_hover("message", |style| style.visible())
                    // At the top of the message, or of what is in view of
                    // it when its top has been scrolled away.
                    .child(in_view(frame_log::timed("actions", actions))),
            )
        })
        .into_any_element()
}
