use std::rc::Rc;

use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App, FontWeight};
use mattermost_api::models::Post;

use super::element_id::eid;
use super::link_preview::link_preview;
use super::markdown_view::markdown;
use super::preview_card::preview_card;
use crate::state::AppState;
use crate::timefmt::format_time;
use crate::ui::message::rules::message_markdown;
use crate::ui::{kit, Action, Ui};

/// The quoted message behind a permalink, as a compact card; or, for an
/// ordinary link, what the server made of it.
pub(super) fn embed_preview(
    ui: &Rc<Ui>,
    index: usize,
    embed: &mattermost_api::models::PostEmbed,
    st: &AppState,
    cx: &App,
) -> Option<AnyElement> {
    if embed.r#type == "opengraph" || embed.r#type == "link" {
        return link_preview(index, embed, cx);
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

    let author = st.author_name(&quoted);
    let channel = st
        .channel(&quoted.channel_id)
        .map(|c| st.channel_title(c))
        .unwrap_or_default();
    let muted = cx.theme().muted_foreground;

    let mut header = h_flex()
        .gap_1p5()
        .items_center()
        .child(kit::avatar(ui, &quoted.user_id, &author, 20.))
        .child(div().font_weight(FontWeight::SEMIBOLD).child(author))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format_time(quoted.create_at)),
        );
    if !channel.is_empty() {
        header = header.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!("in {channel}")),
        );
    }

    // Clicking it goes there, which is what the link would have done.
    let root = quoted.thread_root().to_string();
    Some(
        preview_card(cx)
            .id(("quote", index))
            .cursor_pointer()
            .hover(|style| style.border_color(cx.theme().ring))
            .child(header)
            .child(markdown(
                eid("quoted"),
                message_markdown(&quoted.message, st),
            ))
            .on_click(ui.click(move |ui, cx| {
                ui.dispatch(Action::OpenThread(root.clone()), cx)
            }))
            .into_any_element(),
    )
}
