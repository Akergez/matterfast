//! Joining and leaving teams.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::{dialogs, Action};

impl Ui {
    pub(crate) fn leave_team(self: &Rc<Self>, cx: &mut App) {
        let (client, me, team_id, name) = {
            let st = self.state.borrow();
            let Some(team_id) = st.current_team.clone() else {
                return;
            };
            let name = st
                .teams
                .iter()
                .find(|t| t.id == team_id)
                .map(|t| t.display_name.clone())
                .unwrap_or_default();
            (st.client.clone(), st.me.id.clone(), team_id, name)
        };

        let ui = self.clone();
        dialogs::confirm_leave(self, cx, &name, move |_cx| {
            let client = client.clone();
            let me = me.clone();
            let team_id = team_id.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.leave_team(&team_id, &me).await },
                move |result, cx| match result {
                    Ok(()) => {
                        // Land somewhere real rather than on a team we just
                        // left.
                        ui.reload_teams(cx);
                        let next = ui.state.borrow().teams.first().map(|t| t.id.clone());
                        if let Some(team_id) = next {
                            ui.dispatch(Action::SelectTeam(team_id), cx);
                        }
                    }
                    Err(e) => ui.toast(&format!("Could not leave: {e}"), cx),
                },
            );
        });
    }

    /// Teams on this server you are not in yet.
    pub(crate) fn browse_teams(self: &Rc<Self>, cx: &mut App) {
        let (client, me) = {
            let st = self.state.borrow();
            (st.client.clone(), st.me.id.clone())
        };

        let join_client = client.clone();
        let ui = self.clone();
        let browser = Rc::new(dialogs::TeamBrowser::present(self, cx, move |team_id, _cx| {
            let client = join_client.clone();
            let me = me.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.join_team(&team_id, &me).await },
                move |result, cx| match result {
                    Ok(member) => {
                        ui.reload_teams(cx);
                        ui.dispatch(Action::SelectTeam(member.team_id), cx);
                    }
                    Err(e) => ui.toast(&format!("Could not join: {e}"), cx),
                },
            );
        }));

        let mine = self.state.borrow().teams.clone();
        runtime::spawn(
            async move { client.all_teams(0, 100).await },
            move |result, cx| {
                let Ok(teams) = result else { return };
                let rows = teams
                    .into_iter()
                    .filter(|t| t.delete_at == 0)
                    .map(|team| {
                        let member = mine.iter().any(|m| m.id == team.id);
                        (team.id, team.display_name, team.description, member)
                    })
                    .collect();
                browser.set_teams(rows, cx);
            },
        );
    }
}
