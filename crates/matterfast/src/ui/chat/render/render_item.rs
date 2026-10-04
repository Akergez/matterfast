use std::rc::Rc;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App};

use crate::ui::chat::feed::FeedItem;
use crate::ui::kit::{self, Lucide};
use crate::ui::message::{self, RowOptions};
use crate::ui::Ui;

pub(super) fn render_item(
    ui: &Rc<Ui>,
    item: &FeedItem,
    highlight: Option<&str>,
    cx: &mut App,
) -> AnyElement {
    match item {
        FeedItem::Start(title) => div()
            .px_3()
            .pb_2()
            .text_color(cx.theme().muted_foreground)
            .child(format!("This is the beginning of {title}"))
            .into_any_element(),
        FeedItem::Empty => div()
            .h(px(320.))
            .child(kit::empty_state(
                Lucide::MessageSquarePlus,
                "No messages yet",
                "Say something to get started.",
                cx,
            ))
            .into_any_element(),
        FeedItem::Day(day) => div()
            .px_3()
            .pt_2()
            .child(kit::labelled_rule(day.clone(), cx.theme().border, cx))
            .into_any_element(),
        FeedItem::Unread => div()
            .px_3()
            .pt_2()
            .child(kit::labelled_rule("New messages", cx.theme().danger, cx))
            .into_any_element(),
        FeedItem::System { key, lines } => message::system_block(key, lines, cx),
        FeedItem::Post {
            post,
            grouped,
            body,
        } => message::row(
            ui,
            post,
            body,
            RowOptions {
                grouped: *grouped,
                show_thread_footer: true,
                highlight: highlight == Some(post.id.as_str()),
            },
            cx,
        ),
    }
}
