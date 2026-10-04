use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App, FontWeight};

use super::preview_card::preview_card;

/// A link the server resolved into a title and a description.
pub(super) fn link_preview(
    index: usize,
    embed: &mattermost_api::models::PostEmbed,
    cx: &App,
) -> Option<AnyElement> {
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

    let url = if embed.url.is_empty() {
        read("url")
    } else {
        embed.url.clone()
    };
    let description = read("description");
    let mut card = preview_card(cx).child(div().font_weight(FontWeight::SEMIBOLD).child(title));
    if !description.is_empty() {
        card = card.child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .line_clamp(3)
                .child(description),
        );
    }
    Some(
        card.id(("link", index))
            .cursor_pointer()
            .hover(|style| style.border_color(cx.theme().ring))
            .on_click(move |_, _, cx| crate::open_url(&url, cx))
            .into_any_element(),
    )
}
