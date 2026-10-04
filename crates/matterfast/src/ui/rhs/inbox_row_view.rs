use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App, FontWeight};

use super::inbox_row::InboxRow;
use super::target::Target;
use crate::timefmt::format_relative;
use crate::ui::kit;
use crate::ui::{message, Action, Ui};

pub(super) fn inbox_row_view(ui: &Rc<Ui>, index: usize, row: &InboxRow, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;

    let mut body = v_flex().flex_1().min_w_0().gap_0p5().child(
        h_flex()
            .gap_1p5()
            .items_center()
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(row.author.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .child(row.channel.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(muted)
                    .child(format_relative(row.at)),
            ),
    );
    body = body.child(div().line_clamp(2).child(row.preview.clone()));

    if let Some((replies, unread_replies, unread_mentions)) = row.counts {
        let mut footer = h_flex().gap_1p5().items_center().child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!(
                    "{replies} {}",
                    message::plural(replies, "reply", "replies")
                )),
        );
        if unread_mentions > 0 {
            footer = footer.child(kit::mention_badge(unread_mentions, false, cx));
        } else if unread_replies > 0 {
            footer = footer.child(kit::with_tooltip(
                "unread",
                kit::unread_dot(cx),
                "Unread replies",
            ));
        }
        body = body.child(footer);
    }

    let target = row.target.clone();
    h_flex()
        .id(("inbox-row", index))
        .w_full()
        .items_start()
        .gap_2p5()
        .p_2()
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(theme.list_hover))
        .child(kit::avatar(ui, &row.user_id, &row.author, 32.))
        .child(body)
        // The whole entry is one target.
        .on_click(ui.click(move |ui, cx| {
            ui.dispatch(
                match target.clone() {
                    Target::Thread {
                        channel_id,
                        root_id,
                    } => Action::OpenPost(channel_id, root_id),
                    Target::Message {
                        channel_id,
                        post_id,
                    } => Action::JumpToPost(channel_id, post_id),
                    Target::Followed(root_id) => Action::OpenThread(root_id),
                },
                cx,
            )
        }))
        .into_any_element()
}
