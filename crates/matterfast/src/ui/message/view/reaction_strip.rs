use std::rc::Rc;

use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, FontWeight, SharedString};
use mattermost_api::models::Post;

use super::emoji_element::emoji_element;
use crate::state::AppState;
use crate::ui::message::rules::{reaction_names, reaction_tooltip};
use crate::ui::{Action, Ui};

/// Reactions, collapsed by emoji and drawn as actual emoji rather than
/// `:shortcodes:`. Clicking a chip toggles our own reaction, as everywhere else.
pub(super) fn reaction_strip(
    ui: &Rc<Ui>,
    post: &Post,
    st: &AppState,
    cx: &App,
) -> Option<AnyElement> {
    let reactions = post.reactions();
    if reactions.is_empty() {
        return None;
    }

    // Preserve first-seen order rather than sorting: it matches what the other
    // clients show and keeps chips from jumping around as counts change.
    let mut grouped: Vec<(String, Vec<&mattermost_api::models::Reaction>)> = Vec::new();
    for reaction in reactions {
        match grouped
            .iter_mut()
            .find(|(name, _)| *name == reaction.emoji_name)
        {
            Some((_, group)) => group.push(reaction),
            None => grouped.push((reaction.emoji_name.clone(), vec![reaction])),
        }
    }

    let theme = cx.theme();
    let mut strip = h_flex().flex_wrap().gap_1().mt_1();
    for (index, (name, group)) in grouped.into_iter().enumerate() {
        let mine = group.iter().any(|r| r.user_id == st.me.id);
        let (names, unresolved) = reaction_names(&group, st);
        let tooltip: SharedString = reaction_tooltip(&names, unresolved, &name).into();
        let post_id = post.id.clone();
        let toggle = name.clone();
        strip = strip.child(
            h_flex()
                .id(("reaction", index))
                .gap_1()
                .px_1p5()
                .h(px(24.))
                .items_center()
                .rounded_full()
                .border_1()
                .text_sm()
                .cursor_pointer()
                .border_color(if mine { theme.primary } else { theme.border })
                .bg(if mine {
                    theme.primary.opacity(0.12)
                } else {
                    theme.muted.opacity(0.5)
                })
                .hover(|style| style.border_color(theme.ring))
                .child(emoji_element(ui, &name, 16.))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .child(group.len().to_string()),
                )
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .on_click(ui.click(move |ui, cx| {
                    ui.dispatch(Action::ToggleReaction(post_id.clone(), toggle.clone()), cx)
                })),
        );
    }
    Some(strip.into_any_element())
}
