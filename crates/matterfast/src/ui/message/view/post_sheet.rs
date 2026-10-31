//! What a long press on a message opens on a touch screen: the bar of actions
//! and the "…" menu together, in a sheet from the bottom.
//!
//! A finger has no hover, so the bar a pointer brings up over a message never
//! appears on a phone, and with it go replying and reacting. The press is
//! taken on its way down (capture), ahead of the text view, which would
//! otherwise make it the start of a selection; copying the text is in the
//! sheet instead, the way the official mobile app has it.

use std::rc::Rc;

use gpui_adaptive::prim::{column, pressable, row, testable, Text};
use gpui_adaptive::{ActiveScale as _, Handler, Sheet};
use gpui_kit::prelude::*;
use gpui_kit::{
    canvas, App, Context, DispatchPhase, HitboxBehavior, LongPressEvent, Render, TouchPhase,
    Window,
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
                    || Sheet::is_open(window, cx)
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

/// A press in the sheet: put it away, then do `f`.
fn choose(ui: &Rc<Ui>, f: impl Fn(&Rc<Ui>, &mut App) + 'static) -> Handler {
    let pick = ui.run(f);
    Rc::new(move |window, cx| {
        Sheet::close(window, cx);
        pick(window, cx);
    })
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
    let sheet = PostSheet {
        ui: ui.clone(),
        post_id: post_id.to_string(),
        entries,
        quick_reactions,
    };
    Sheet::open(cx.new(|_| sheet), window, cx);
}

/// The reactions most used, then every entry, in the sheet.
struct PostSheet {
    ui: Rc<Ui>,
    post_id: String,
    entries: Vec<(&'static str, Choice)>,
    quick_reactions: Vec<String>,
}

impl Render for PostSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (ui, post_id) = (&self.ui, &self.post_id);
        let scale = cx.scale();
        let mut quick = row().flex_wrap().gap_1().px(scale.space_m);
        for (index, name) in self.quick_reactions.iter().enumerate() {
            let pick = choose(ui, {
                let (post_id, name) = (post_id.clone(), name.clone());
                move |ui, cx| ui.dispatch(Action::ToggleReaction(post_id.clone(), name.clone()), cx)
            });
            quick = quick.child(testable(
                pressable(("quick", index), cx)
                    .justify_center()
                    .size(scale.target)
                    .rounded(scale.radius_control)
                    .text_size(scale.title)
                    .child(emoji::label(name))
                    .on_click(move |_, window, cx| pick(window, cx)),
            ));
        }
        let more = post_id.clone();
        quick = quick.child(
            kit::icon_button("more", Lucide::Search, "Search every emoji")
                .on_trigger({
                    let pick = choose(ui, move |ui, cx| ui.pick_reaction(more.clone(), cx));
                    move |window, cx| pick(window, cx)
                })
                .as_icon_button(),
        );

        let mut list = column();
        for (index, (label, choice)) in self.entries.iter().enumerate() {
            let choice = choice.clone();
            let post_id = post_id.clone();
            let pick = choose(ui, move |ui, cx| match &choice {
                Choice::Reply(root) => ui.dispatch(Action::OpenThread(root.clone()), cx),
                Choice::Post(action) => ui.dispatch(Action::Post(post_id.clone(), *action), cx),
            });
            list = list.child(testable(
                pressable(("entry", index), cx)
                    .w_full()
                    .min_h(scale.row)
                    .px(scale.space_l)
                    .child(Text::body(*label))
                    .on_click(move |_, window, cx| pick(window, cx)),
            ));
        }

        column().gap(scale.space_s).pb(scale.space_s).child(quick).child(list)
    }
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
