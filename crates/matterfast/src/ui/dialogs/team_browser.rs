use std::rc::Rc;

use gpui_kit::{App, Entity};

use super::list::{open_list, List, Row};
use crate::ui::kit::Lucide;
use crate::ui::Ui;

/// Teams you could join, filled in by [`Self::set_teams`].
///
/// There is no search: an account sees the teams it is allowed to join and
/// that list is short enough to read.
pub struct TeamBrowser {
    list: Option<Entity<List>>,
    on_join: Rc<dyn Fn(String, &mut App)>,
}

impl TeamBrowser {
    pub fn present(ui: &Rc<Ui>, cx: &mut App, on_join: impl Fn(String, &mut App) + 'static) -> Self {
        let list = open_list(ui, cx, "Browse teams", |list, _, _| {
            list.section("", Some((Lucide::Users, "No teams to join", "")));
        });
        TeamBrowser {
            list,
            on_join: Rc::new(on_join),
        }
    }

    /// `teams` is (id, display_name, description, already_member).
    pub fn set_teams(&self, teams: Vec<(String, String, String, bool)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        let rows = teams
            .into_iter()
            .map(|(id, display_name, description, already_member)| {
                let row = Row::new(id.clone(), display_name).subtitle(description);
                if already_member {
                    row.label("Joined")
                } else {
                    let on_join = self.on_join.clone();
                    row.button("Join", "Joined", move |cx| on_join(id.clone(), cx))
                }
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }
}
