use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, FontWeight};

use super::card_action::{card_action, card_color};
use super::element_id::eid;
use super::markdown_view::markdown;
use crate::ui::Ui;

/// One rich card: a coloured stripe, a title that may be a link, some text,
/// and its fields laid out as label-and-value rows.
pub(super) fn attachment_card(
    ui: &Rc<Ui>,
    post_id: &str,
    index: usize,
    card: &mattermost_api::models::MessageAttachment,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut content = v_flex().flex_1().min_w_0().gap_0p5().pl_2p5();
    let mut empty = true;

    if !card.author_name.is_empty() {
        content = content.child(div().text_xs().text_color(muted).child(card.author_name.clone()));
        empty = false;
    }
    if !card.pretext.is_empty() {
        content = content.child(div().text_color(muted).child(card.pretext.clone()));
        empty = false;
    }
    if !card.title.is_empty() {
        // A title with a link is a link; without one it is just bold.
        let title = div().font_weight(FontWeight::SEMIBOLD);
        content = content.child(if card.title_link.is_empty() {
            title.child(card.title.clone()).into_any_element()
        } else {
            let url = card.title_link.clone();
            title
                .id("title")
                .text_color(cx.theme().link)
                .cursor_pointer()
                .hover(|style| style.underline())
                .child(card.title.clone())
                .on_click(move |_, _, cx| crate::open_url(&url, cx))
                .into_any_element()
        });
        empty = false;
    }
    if !card.text.is_empty() {
        content = content.child(markdown(
            eid("text"),
            crate::markdown::prepare_with(&card.text, &|_| None, crate::markdown::Sigil::Keep),
        ));
        empty = false;
    }

    for field in &card.fields {
        let value = field.text();
        if field.title.is_empty() && value.is_empty() {
            continue;
        }
        let mut row = v_flex().mt_1();
        if !field.title.is_empty() {
            row = row.child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(field.title.clone()),
            );
        }
        if !value.is_empty() {
            row = row.child(div().child(value));
        }
        content = content.child(row);
        empty = false;
    }

    if !card.actions.is_empty() {
        let mut row = h_flex().mt_1p5().gap_1p5().flex_wrap();
        for action in card.actions.iter().filter(|action| !action.id.is_empty()) {
            row = row.child(card_action(ui, post_id, index, action, cx));
        }
        content = content.child(row);
        empty = false;
    }

    if !card.footer.is_empty() {
        content = content.child(div().text_xs().text_color(muted).child(card.footer.clone()));
        empty = false;
    }
    // Nothing usable in the card itself: the fallback is what it is for.
    if empty && !card.fallback.is_empty() {
        content = content.child(div().child(card.fallback.clone()));
    }

    // The sender's colour, where they gave one — it usually encodes the status
    // of whatever the card reports, so it carries meaning. Without one, a
    // muted version of the text colour, so it still reads as a card.
    let stripe = card_color(&card.color, cx).unwrap_or(cx.theme().foreground.opacity(0.3));
    h_flex()
        .id(("card", index))
        .w_full()
        .items_stretch()
        .mt_1()
        .child(div().flex_none().w(px(3.)).rounded_sm().bg(stripe))
        .child(content)
        .into_any_element()
}
