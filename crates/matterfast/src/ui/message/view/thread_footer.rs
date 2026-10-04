use std::rc::Rc;

use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, FontWeight};
use mattermost_api::models::Post;

use crate::state::AppState;
use crate::ui::message::rules::plural;
use crate::ui::{kit, Action, Ui};

/// How many repliers to show before the count speaks for itself.
const THREAD_FACES: usize = 5;

/// The "N replies" footer: who replied, then the count.
pub(super) fn thread_footer(
    ui: &Rc<Ui>,
    post: &Post,
    st: &AppState,
    cx: &gpui_kit::App,
) -> AnyElement {
    let label = format!(
        "{} {}",
        post.reply_count,
        plural(post.reply_count, "reply", "replies")
    );
    // Who replied, before the count: a thread is worth opening because of
    // who is in it, and the faces answer that before the number does.
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

    let mut footer = h_flex().gap_1p5().items_center().mt_0p5();
    for user_id in repliers.iter().take(THREAD_FACES) {
        let name = st
            .users
            .get(user_id)
            .map(|u| u.display_name(&display))
            .unwrap_or_default();
        footer = footer.child(kit::avatar(ui, user_id, &name, 20.));
    }

    let root = post.thread_root().to_string();
    footer
        .child(
            div()
                .id("replies")
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().link)
                .cursor_pointer()
                .hover(|style| style.underline())
                .child(label)
                .on_click(
                    ui.click(move |ui, cx| ui.dispatch(Action::OpenThread(root.clone()), cx)),
                ),
        )
        .into_any_element()
}
