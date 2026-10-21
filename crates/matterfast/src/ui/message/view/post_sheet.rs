//! What a long press on a message opens on a touch screen: the bar of actions
//! and the "…" menu together, in a sheet from the bottom.
//!
//! A finger has no hover, so the bar a pointer brings up over a message never
//! appears on a phone, and with it go replying and reacting. The press is
//! taken on its way down (capture), ahead of the text view, which would
//! otherwise make it the start of a selection; copying the text is in the
//! sheet instead, the way the official mobile app has it.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Placement, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{
    canvas, div, rems, App, ClickEvent, DispatchPhase, HitboxBehavior, LongPressEvent,
    TouchPhase, Window,
};
use mattermost_api::models::Post;

use super::post_menu::post_menu_entries;
use crate::emoji;
use crate::ui::message::PostAction;
use crate::ui::{kit, Action, Ui};
use kit::Lucide;

#[derive(Debug, Clone, PartialEq)]
enum Choice {
    /// Open the thread, by its root.
    Reply(String),
    Post(PostAction),
}

/// What the sheet lists under the reactions: what the bar holds, then what
/// the menu holds, in that order.
fn sheet_entries(
    post: &Post,
    mine: bool,
    saved: bool,
    allow_thread: bool,
) -> Vec<(&'static str, Choice)> {
    let mut entries = Vec::new();
    if allow_thread {
        entries.push(("Reply", Choice::Reply(post.thread_root().to_string())));
    }
    entries.push(if saved {
        ("Remove from saved", Choice::Post(PostAction::Unsave))
    } else {
        ("Save", Choice::Post(PostAction::Save))
    });
    entries.extend(
        post_menu_entries(post, mine)
            .into_iter()
            .map(|(label, action)| (label, Choice::Post(action))),
    );
    entries
}

/// Laid over a whole message row, under everything in it: it only listens
/// for a long press inside the row.
pub(super) fn long_press_area(ui: &Rc<Ui>, post_id: &str, allow_thread: bool) -> impl IntoElement {
    let ui = ui.clone();
    let post_id = post_id.to_string();
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            window.on_mouse_event(move |event: &LongPressEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture
                    || event.phase != TouchPhase::Started
                    || !hitbox.bounds.contains(&event.start_position)
                    || window.has_active_dialog(cx)
                    || window.has_active_sheet(cx)
                {
                    return;
                }
                window.prevent_default();
                cx.stop_propagation();
                open(&ui, &post_id, allow_thread, window, cx);
            });
        },
    )
    .absolute()
    .size_full()
}

/// A click in the sheet: put it away, then do `f`.
fn choose(
    ui: &Rc<Ui>,
    f: impl Fn(&Rc<Ui>, &mut App) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let pick = ui.click(f);
    move |event, window, cx| {
        window.close_sheet(cx);
        pick(event, window, cx);
    }
}

fn open(ui: &Rc<Ui>, post_id: &str, allow_thread: bool, window: &mut Window, cx: &mut App) {
    let (entries, quick_reactions) = {
        let st = ui.state.borrow();
        let Some(post) = st.post(post_id) else {
            return;
        };
        let mine = post.user_id == st.me.id;
        let saved = st.saved_posts.contains(post_id);
        (sheet_entries(&post, mine, saved, allow_thread), st.quick_reactions())
    };
    let ui = ui.clone();
    let post_id = post_id.to_string();
    // Tall enough for the reactions and every entry, each a line and its
    // padding; a short screen scrolls the rest.
    let height = rems(5.5 + 2.5 * entries.len() as f32);
    window.open_sheet_at(Placement::Bottom, cx, move |sheet, _, cx| {
        let hover = cx.theme().list_hover;

        let mut quick = h_flex().flex_wrap().gap_1();
        for (index, name) in quick_reactions.iter().enumerate() {
            let name = name.clone();
            let post_id = post_id.clone();
            quick = quick.child(
                Button::new(("quick", index))
                    .ghost()
                    .label(emoji::label(&name))
                    .on_click(choose(&ui, move |ui, cx| {
                        ui.dispatch(Action::ToggleReaction(post_id.clone(), name.clone()), cx)
                    })),
            );
        }
        let more = post_id.clone();
        quick = quick.child(
            kit::icon_button("more", Lucide::Search, "Search every emoji")
                .on_click(choose(&ui, move |ui, cx| ui.pick_reaction(more.clone(), cx))),
        );

        let mut list = v_flex();
        for (index, (label, choice)) in entries.iter().enumerate() {
            let post_id = post_id.clone();
            let choice = choice.clone();
            list = list.child(
                div()
                    .id(("entry", index))
                    .w_full()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(hover))
                    .active(|style| style.bg(hover))
                    .child(*label)
                    .on_click(choose(&ui, move |ui, cx| match &choice {
                        Choice::Reply(root) => ui.dispatch(Action::OpenThread(root.clone()), cx),
                        Choice::Post(action) => {
                            ui.dispatch(Action::Post(post_id.clone(), *action), cx)
                        }
                    })),
            );
        }

        sheet
            .size(height)
            .resizable(false)
            .child(v_flex().gap_2().child(quick).child(list))
    });
}

#[cfg(test)]
mod tests {
    use mattermost_api::models::Post;

    use super::{sheet_entries, Choice};
    use crate::ui::message::PostAction;

    fn post() -> Post {
        Post {
            id: "p".into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_sheet_starts_with_reply_and_save_where_the_bar_has_them() {
        let entries = sheet_entries(&post(), false, false, true);
        assert_eq!(entries[0], ("Reply", Choice::Reply("p".into())));
        assert_eq!(entries[1], ("Save", Choice::Post(PostAction::Save)));
    }

    #[test]
    fn a_saved_message_offers_to_unsave_it() {
        let entries = sheet_entries(&post(), false, true, true);
        assert!(entries.contains(&("Remove from saved", Choice::Post(PostAction::Unsave))));
    }

    #[test]
    fn no_reply_where_there_is_no_thread_to_open() {
        let entries = sheet_entries(&post(), false, false, false);
        assert!(!entries.iter().any(|(_, choice)| matches!(choice, Choice::Reply(_))));
    }

    #[test]
    fn the_sheet_holds_everything_the_menu_does() {
        let entries = sheet_entries(&post(), true, false, true);
        for action in [PostAction::CopyText, PostAction::CopyLink, PostAction::Edit, PostAction::Delete] {
            assert!(entries.iter().any(|(_, choice)| *choice == Choice::Post(action)));
        }
    }
}
