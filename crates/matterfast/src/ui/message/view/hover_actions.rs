use std::rc::Rc;

use gpui_adaptive::prim::{pressable, row, tooltip};
use gpui_adaptive::{ActiveColors as _, ActiveScale as _, Popup};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App};
use mattermost_api::models::Post;

use super::post_menu::overflow_menu;
use crate::emoji;
use crate::ui::message::PostAction;
use crate::ui::{kit, Action, Ui};
use kit::Lucide;

/// The small react / save / reply buttons that appear over a row on hover.
pub(super) fn hover_actions(
    ui: &Rc<Ui>,
    post: &Post,
    allow_thread: bool,
    mine: bool,
    saved: bool,
    cx: &App,
) -> AnyElement {
    let post_id = post.id.clone();

    // The latest used sit in the popover, because most reactions are one of
    // them; the full table is one more click away.
    let quick_reactions = ui.state.borrow().quick_reactions();
    let react = Popup::new(
        "react",
        kit::icon_button("react-button", Lucide::FaceSlightlySmilingPlus, "Add reaction"),
        {
            let ui = ui.clone();
            let post_id = post_id.clone();
            move |close, _, cx| {
                // Picking is the end of the gesture: the palette goes away
                // with the choice rather than waiting for a click elsewhere.
                let mut quick = row().gap_0p5();
                for (index, name) in quick_reactions.iter().enumerate() {
                    let pick = ui.run({
                        let (post_id, name) = (post_id.clone(), name.clone());
                        move |ui, cx| ui.dispatch(Action::ToggleReaction(post_id.clone(), name.clone()), cx)
                    });
                    let close = close.clone();
                    quick = quick.child(
                        pressable(("quick", index), cx)
                            .justify_center()
                            .size(cx.scale().target)
                            .rounded(cx.scale().radius_control)
                            .text_size(cx.scale().title)
                            .child(emoji::label(name))
                            .tooltip(tooltip(format!(":{name}:")))
                            .on_click(move |_, window, cx| {
                                close(window, cx);
                                pick(window, cx);
                            }),
                    );
                }
                let post_id = post_id.clone();
                let pick = ui.run(move |ui, cx| ui.pick_reaction(post_id.clone(), cx));
                let close = close.clone();
                quick
                    .child(
                        kit::icon_button("more", Lucide::Search, "Search every emoji")
                            .on_trigger(move |window, cx| {
                                close(window, cx);
                                pick(window, cx);
                            })
                            .as_icon_button(),
                    )
                    .into_any_element()
            }
        },
    );

    let colors = cx.colors();
    let mut bar = row()
        .gap_0p5()
        .p_0p5()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.raised)
        .shadow_sm()
        .child(react);

    // Saving is one click in every other client, so it is a button here too
    // rather than being buried in the menu.
    bar = bar.child(
        kit::icon_button(
            "save",
            if saved { Lucide::BookmarkCheck } else { Lucide::Bookmark },
            if saved { "Remove from saved" } else { "Save" },
        )
        .on_trigger(ui.run({
            let post_id = post_id.clone();
            move |ui, cx| {
                ui.dispatch(
                    Action::Post(
                        post_id.clone(),
                        if saved { PostAction::Unsave } else { PostAction::Save },
                    ),
                    cx,
                )
            }
        }))
        .as_icon_button(),
    );

    if allow_thread {
        let root = post.thread_root().to_string();
        bar = bar.child(
            kit::icon_button("reply", Lucide::Reply, "Reply in thread").on_trigger(ui.run(
                move |ui, cx| ui.dispatch(Action::OpenThread(root.clone()), cx),
            )).as_icon_button(),
        );
    }

    bar.child(overflow_menu(ui, post, mine))
        .into_any_element()
}
