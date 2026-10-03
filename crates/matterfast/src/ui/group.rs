//! A mentioned group: who is in it, which is what pressing the mention shows.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, ElementId, FontWeight};
use mattermost_api::models::{Group, User};

use super::kit;
use super::Ui;
use crate::runtime;

/// The people of a group, as far as the server has been asked.
enum Roster {
    Loading,
    Here(Vec<User>),
    Failed,
}

/// "@developers · 12 people", or as much of it as is known.
pub fn summary(group: &Group) -> String {
    match group.member_count {
        Some(1) => format!("@{} · 1 person", group.name),
        Some(count) => format!("@{} · {count} people", group.name),
        None => format!("@{}", group.name),
    }
}

/// Shows the group and its people. They are asked for when it opens: who is
/// in a group is not something the client holds.
pub fn show(ui: &Rc<Ui>, group: Group, cx: &mut App) {
    let roster = Rc::new(RefCell::new(Roster::Loading));

    let client = ui.state.borrow().client.clone();
    let group_id = group.id.clone();
    runtime::spawn(
        async move {
            const PAGE: u32 = 200;
            // A group of two thousand is not read as a list anyway; stop.
            const PAGES: u32 = 10;
            let mut members = Vec::new();
            for page in 0..PAGES {
                let batch = client.group_members(&group_id, page, PAGE).await?.members;
                let last = (batch.len() as u32) < PAGE;
                members.extend(batch);
                if last {
                    break;
                }
            }
            Ok::<_, mattermost_api::Error>(members)
        },
        {
            let (ui, roster) = (ui.clone(), roster.clone());
            move |result, cx| {
                *roster.borrow_mut() = match result {
                    Ok(mut members) => {
                        let mut st = ui.state.borrow_mut();
                        let display = st.teammate_name_display().to_string();
                        members.sort_by_cached_key(|user| user.display_name(&display).to_lowercase());
                        // Kept, so that a row can show its picture and
                        // presence, and open the profile, like anybody else.
                        for user in &members {
                            st.users
                                .entry(user.id.clone())
                                .or_insert_with(|| user.clone());
                        }
                        Roster::Here(members)
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "could not list the members of a group");
                        Roster::Failed
                    }
                };
                cx.refresh_windows();
            }
        },
    );

    let card_ui = ui.clone();
    ui.with_window(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, _, cx| {
            dialog
                .w(px(340.))
                .child(card(&card_ui, &group, &roster.borrow(), cx))
        });
    });
}

fn card(ui: &Rc<Ui>, group: &Group, roster: &Roster, cx: &App) -> gpui_kit::AnyElement {
    let muted = cx.theme().muted_foreground;
    let heading = v_flex()
        .gap_0p5()
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(group.display_name.clone()),
        )
        .child(div().text_sm().text_color(muted).child(match roster {
            // The list is here, so its length is the number to trust.
            Roster::Here(members) => summary(&Group {
                member_count: Some(members.len() as i64),
                ..group.clone()
            }),
            _ => summary(group),
        }));

    let body = match roster {
        Roster::Loading => div()
            .py_4()
            .text_sm()
            .text_color(muted)
            .child("Loading…")
            .into_any_element(),
        Roster::Failed => div()
            .py_4()
            .text_sm()
            .text_color(muted)
            .child("The members could not be loaded.")
            .into_any_element(),
        Roster::Here(members) if members.is_empty() => div()
            .py_4()
            .text_sm()
            .text_color(muted)
            .child("Nobody is in this group.")
            .into_any_element(),
        Roster::Here(members) => {
            let st = ui.state.borrow();
            let hover = cx.theme().list_hover;
            let mut list = v_flex().gap_0p5();
            for user in members {
                let name = st.display_name(user);
                let presence = st.presence(&user.id);
                let id = user.id.clone();
                list = list.child(
                    h_flex()
                        .id(ElementId::Name(format!("group-member-{id}").into()))
                        .px_1()
                        .py_1()
                        .gap_2()
                        .items_center()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .child(kit::avatar_with_presence(ui, &user.id, &name, 28., presence, cx))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().truncate().child(name))
                                .child(
                                    div()
                                        .truncate()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(format!("@{}", user.username)),
                                ),
                        )
                        .on_click({
                            let ui = ui.clone();
                            move |_, window, cx| {
                                window.close_dialog(cx);
                                let id = id.clone();
                                ui.later(cx, move |ui, cx| ui.show_profile(&id, cx));
                            }
                        }),
                );
            }
            div()
                .id("group-members")
                .max_h(px(420.))
                .overflow_y_scroll()
                .child(list)
                .into_any_element()
        }
    };

    v_flex().gap_3().child(heading).child(body).into_any_element()
}
