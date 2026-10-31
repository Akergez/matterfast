use gpui_adaptive::prim::{self, column, pressable, row, Scrolls as _};
use gpui_adaptive::{ActiveColors as _, Avatar, Command, Popup};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, px, rems, AnyElement, App, ElementId, FontWeight};

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
    // A choice made in it closes it, so that it registers as made.
    Popup::new("account", Command::new("account-button", "Account and teams"), move |close, _, cx| {
            let ui = &content_ui;
            let st = ui.state.borrow();
            let colors = cx.colors();
            let me = &st.me;
            let display = me.display_name(st.teammate_name_display());

            let account = row()
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
                    column()
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
                                .text_color(colors.muted)
                                .child(format!("@{}", me.username)),
                        ),
                );

            // Setting your own status belongs with your own name, which is
            // here.
            let mut statuses = row().gap_1();
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
                statuses = statuses.child(prim::testable(
                    pressable(("status", index), cx)
                        .flex_1()
                        .h(px(28.))
                        .justify_center()
                        .rounded_md()
                        .tooltip(prim::tooltip(label))
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
                ));
            }

            let mut teams = column().gap_0p5();
            for team in st.teams.iter().filter(|t| t.delete_at == 0) {
                let current = st.current_team.as_deref() == Some(team.id.as_str());
                let close = close.clone();
                let ui = ui.clone();
                let team_id = team.id.clone();
                let mut row = pressable(ElementId::Name(format!("team-{}", team.id).into()), cx)
                    .h(px(34.))
                    .px_2()
                    .gap_3()
                    .rounded_md()
                    .when(current, |row| row.bg(colors.hover))
                    .child(
                        Avatar::new(ElementId::Name(format!("team-face-{}", team.id).into()), kit::initials(&team.display_name))
                            .size(rems(1.5))
                            .square(),
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
                teams = teams.child(prim::testable(row.on_click(move |_, window, cx| {
                    close(window, cx);
                    ui.dispatch(Action::SelectTeam(team_id.clone()), cx);
                })));
            }

            column()
                .w(px(260.))
                .gap_2()
                .child(account)
                .child(statuses)
                .child(div().h(px(1.)).bg(colors.border))
                .child(
                    div()
                        .px_1()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.muted)
                        .child("TEAMS"),
                )
                .child(
                    div()
                        .id("teams")
                        .max_h(px(320.))
                        .scrolls_y()
                        .child(teams),
                )
                .into_any_element()
    })
    .face(kit::avatar_with_presence(ui, &me_id, &name, 24., presence, cx))
    .into_any_element()
}
