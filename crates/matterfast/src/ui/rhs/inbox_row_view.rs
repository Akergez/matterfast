use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App, FontWeight};

use super::inbox_row::InboxRow;
use super::panel_mode::PanelMode;
use super::target::Target;
use crate::ui::sidebar::{
    INBOX_ANSWERS as ANSWERS, INBOX_FACE, INBOX_HEADING as HEADING, INBOX_ROW, INBOX_SAID as SAID,
};
use crate::timefmt::format_relative;
use crate::ui::kit;
use crate::ui::{message, Action, Ui};

pub(super) fn inbox_row_view(ui: &Rc<Ui>, index: usize, row: &InboxRow, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;

    // The three lines are set tight, each to a height of its own, so that
    // together they are a little shorter than the face beside them: the face
    // is what the row is as tall as, and the text does not stand out past
    // it at the top or the bottom.
    let mut body = v_flex().flex_1().min_w_0().child(
        h_flex()
            .h(HEADING)
            .line_height(HEADING)
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
            // Why an old message is in a list of new things.
            .when(row.saved, |heading| {
                heading.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(muted)
                        .child(kit::Lucide::Bookmark),
                )
            })
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(muted)
                    .child(format_relative(row.at)),
            ),
    );
    // One line of what was said: the row is something to recognise a thread
    // by, and the thread is a press away.
    body = body.child(
        div()
            .h(SAID)
            .line_height(SAID)
            .truncate()
            .text_sm()
            // Quieter than who said it: the name is what the row is found
            // by.
            .text_color(muted)
            .child(row.preview.clone()),
    );

    // The third line is there for every row, so that all of them are one
    // height. Only a followed thread comes with its numbers; of the others
    // it is known what they are.
    let (replies, unread_replies, unread_mentions) = row.counts.unwrap_or_default();
    let answers = match (&row.target, row.counts) {
        (_, Some(_)) if replies > 0 => {
            format!("{replies} {}", message::plural(replies, "reply", "replies"))
        }
        (Target::Thread { .. }, None) => "A reply in a thread".to_string(),
        _ => "No replies".to_string(),
    };
    // A badge is taller than the line it is on, and hangs over it rather
    // than push the row apart.
    let mut footer = h_flex()
        .h(ANSWERS)
        .line_height(ANSWERS)
        .gap_1p5()
        .items_center()
        .child(div().text_xs().text_color(muted).child(answers));
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

    // The thread open beside the conversation is the row that is lit, the
    // way the open conversation is in a list of them.
    let selected = match (ui.right.mode(cx), &row.target) {
        (
            PanelMode::Thread(open),
            Target::Followed { root_id, .. } | Target::Thread { root_id, .. },
        ) => open == *root_id,
        _ => false,
    };
    let target = row.target.clone();
    h_flex()
        .id(("inbox-row", index))
        .w_full()
        .h(INBOX_ROW)
        .items_center()
        .gap_2p5()
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.sidebar_accent))
        .when(!selected, |row| row.hover(|style| style.bg(theme.list_hover)))
        // A little taller than the three lines beside it.
        .child(kit::avatar(
            ui,
            &row.user_id,
            &row.author,
            f32::from(INBOX_FACE.to_pixels(theme.font_size)),
        ))
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
                    // A thread whose channel is not known — the server left
                    // it out — still opens, beside what is on screen.
                    Target::Followed { channel_id, root_id } if channel_id.is_empty() => {
                        Action::OpenThread(root_id)
                    }
                    Target::Followed { channel_id, root_id } => {
                        Action::OpenPost(channel_id, root_id)
                    }
                },
                cx,
            )
        }))
        .into_any_element()
}
