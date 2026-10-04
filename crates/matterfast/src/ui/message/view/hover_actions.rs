use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{h_flex, ActiveTheme, Sizable};
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

    // The eight most-used sit in the popover, because most reactions are one
    // of them; the full table is one more click away.
    let react = Popover::new("react")
        .trigger(kit::icon_button("react-button", Lucide::FaceSlightlySmilingPlus, "Add reaction"))
        .content({
            let ui = ui.clone();
            let post_id = post_id.clone();
            move |_, _, cx| {
                // Picking is the end of the gesture: the palette goes away
                // with the choice rather than waiting for a click elsewhere.
                let popover = cx.entity();
                let mut quick = h_flex().gap_0p5();
                for (index, name) in emoji::QUICK_REACTIONS.iter().enumerate() {
                    let name = (*name).to_string();
                    let post_id = post_id.clone();
                    quick = quick.child(
                        Button::new(("quick", index))
                            .ghost()
                            .small()
                            .label(emoji::label(&name))
                            .tooltip(format!(":{name}:"))
                            .on_click({
                                let popover = popover.clone();
                                let pick = ui.click(move |ui, cx| {
                                    ui.dispatch(
                                        Action::ToggleReaction(post_id.clone(), name.clone()),
                                        cx,
                                    )
                                });
                                move |event, window, cx| {
                                    popover.update(cx, |state, cx| state.dismiss(window, cx));
                                    pick(event, window, cx);
                                }
                            }),
                    );
                }
                let post_id = post_id.clone();
                quick
                    .child(
                        kit::icon_button("more", Lucide::Search, "Search every emoji").on_click({
                            let pick = ui.click(move |ui, cx| ui.pick_reaction(post_id.clone(), cx));
                            move |event, window, cx| {
                                popover.update(cx, |state, cx| state.dismiss(window, cx));
                                pick(event, window, cx);
                            }
                        }),
                    )
                    .into_any_element()
            }
        });

    let mut bar = h_flex()
        .gap_0p5()
        .p_0p5()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
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
        .on_click(ui.click({
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
        })),
    );

    if allow_thread {
        let root = post.thread_root().to_string();
        bar = bar.child(
            kit::icon_button("reply", Lucide::Reply, "Reply in thread").on_click(ui.click(
                move |ui, cx| ui.dispatch(Action::OpenThread(root.clone()), cx),
            )),
        );
    }

    bar.child(overflow_menu(ui, post, mine))
        .into_any_element()
}
