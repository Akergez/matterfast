use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, ElementId, FontWeight, Window};

use crate::ui::kit;
use crate::ui::{Action, Ui};

/// The round avatar in the sidebar header and what drops out of it: who you
/// are signed in as, and the teams to switch between. A rail of teams would
/// cost a permanent column to say what a popover says on demand.
pub(super) fn switcher(ui: &Rc<Ui>, cx: &App) -> AnyElement {
    let (me_id, name, presence) = {
        let st = ui.state.borrow();
        (
            st.me.id.clone(),
            st.me.display_name(st.teammate_name_display()),
            st.presence(&st.me.id),
        )
    };
    let content_ui = ui.clone();
    Popover::new("account")
        .trigger(
            Button::new("account-button")
                .ghost()
                .small()
                .tooltip("Account and teams")
                .child(kit::avatar_with_presence(
                    ui, &me_id, &name, 24., presence, cx,
                )),
        )
        .content(move |_, _, cx| {
            let ui = &content_ui;
            let popover = cx.entity();
            // Close the popover a button lives in, so the choice registers
            // as made.
            let close = move |window: &mut Window, cx: &mut App| {
                popover.update(cx, |state, cx| state.dismiss(window, cx));
            };
            let st = ui.state.borrow();
            let theme = cx.theme();
            let me = &st.me;
            let display = me.display_name(st.teammate_name_display());

            let account = h_flex()
                .gap_3()
                .items_center()
                .child(kit::avatar_with_presence(
                    ui,
                    &me.id,
                    &display,
                    40.,
                    st.presence(&me.id),
                    cx,
                ))
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(display.clone()),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("@{}", me.username)),
                        ),
                );

            // Setting your own status belongs with your own name, which is
            // here.
            let mut statuses = h_flex().gap_1();
            for (index, (label, value)) in [
                ("Online", "online"),
                ("Away", "away"),
                ("Do not disturb", "dnd"),
                ("Offline", "offline"),
            ]
            .into_iter()
            .enumerate()
            {
                let close = close.clone();
                let ui = ui.clone();
                statuses = statuses.child(
                    Button::new(("status", index))
                        .ghost()
                        .small()
                        .flex_1()
                        .tooltip(label)
                        .child(
                            div().size(px(10.)).rounded_full().bg(kit::presence_color(
                                mattermost_api::models::Presence::from(value),
                                cx,
                            )),
                        )
                        .on_click(move |_, window, cx| {
                            close(window, cx);
                            ui.dispatch(Action::SetStatus(value.to_string()), cx);
                        }),
                );
            }

            let mut teams = v_flex().gap_0p5();
            for team in st.teams.iter().filter(|t| t.delete_at == 0) {
                let current = st.current_team.as_deref() == Some(team.id.as_str());
                let close = close.clone();
                let ui = ui.clone();
                let team_id = team.id.clone();
                let mut row = h_flex()
                    .id(ElementId::Name(format!("team-{}", team.id).into()))
                    .h(px(34.))
                    .px_2()
                    .gap_3()
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .when(current, |row| row.bg(theme.accent))
                    .hover(|style| style.bg(theme.list_hover))
                    .child(
                        gpui_kit::component::avatar::Avatar::new()
                            .name(team.display_name.clone())
                            .with_size(gpui_kit::component::Size::Size(px(24.))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(team.display_name.clone()),
                    );
                // Where something is waiting, so switching teams is a
                // decision rather than a guess.
                if let Some((messages, mentions)) = st.team_unreads.get(&team.id) {
                    if *mentions > 0 {
                        row = row.child(kit::mention_badge(*mentions, false, cx));
                    } else if *messages > 0 {
                        row = row.child(kit::unread_dot(cx));
                    }
                }
                teams = teams.child(row.on_click(move |_, window, cx| {
                    close(window, cx);
                    ui.dispatch(Action::SelectTeam(team_id.clone()), cx);
                }));
            }

            v_flex()
                .w(px(260.))
                .gap_2()
                .child(account)
                .child(statuses)
                .child(div().h(px(1.)).bg(theme.border))
                .child(
                    div()
                        .px_1()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.muted_foreground)
                        .child("TEAMS"),
                )
                .child(
                    div()
                        .id("teams")
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .child(teams),
                )
        })
        .into_any_element()
}
