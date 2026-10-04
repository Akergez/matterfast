use std::rc::Rc;

use gpui_kit::{App, Entity};

use super::list::{open_list, List, Row};
use crate::ui::kit::Lucide;
use crate::ui::Ui;

/// Who is in a channel, with a search for adding more.
///
/// Like [`super::ChannelBrowser`] the dialog outlives this call: the caller
/// answers `on_search` through [`Self::set_candidates`], and fills the roster
/// with [`Self::set_members`].
pub struct MemberList {
    list: Option<Entity<List>>,
    on_add: Rc<dyn Fn(String, &mut App)>,
    on_remove: Rc<dyn Fn(String, &mut App)>,
}

impl MemberList {
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        channel_name: &str,
        on_search: impl Fn(String, &mut App) + 'static,
        on_add: impl Fn(String, &mut App) + 'static,
        on_remove: impl Fn(String, &mut App) + 'static,
    ) -> Self {
        let on_search = Rc::new(on_search);
        let typed = on_search.clone();
        let list = open_list(
            ui,
            cx,
            &format!("Members of {channel_name}"),
            move |list, window, cx| {
                list.section(
                    "Members",
                    Some((Lucide::Users, "Nobody here yet", "")),
                );
                let candidates = list.section(
                    "Add people",
                    Some((Lucide::Search, "No matching people", "")),
                );
                list.searchable(
                    candidates,
                    "Search people",
                    move |term, cx| typed(term, cx),
                    window,
                    cx,
                );
            },
        );
        // Ask once on open, as the channel browser does: the people you want
        // to add are usually the ones the server would have listed anyway.
        cx.defer(move |cx| on_search(String::new(), cx));
        MemberList {
            list,
            on_add: Rc::new(on_add),
            on_remove: Rc::new(on_remove),
        }
    }

    /// `members` is (user_id, display_name, @username, is_admin).
    pub fn set_members(&self, members: Vec<(String, String, String, bool)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        let rows = members
            .into_iter()
            .map(|(id, display_name, username, is_admin)| {
                let row = Row::new(id.clone(), display_name).subtitle(username);
                if is_admin {
                    // Removing an admin needs permissions we cannot check from
                    // here, so the row explains itself instead of offering a
                    // button the server would refuse.
                    row.label("Admin")
                } else {
                    let on_remove = self.on_remove.clone();
                    row.icon_button(Lucide::UserMinus, "Remove from channel", move |cx| {
                        on_remove(id.clone(), cx)
                    })
                }
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }

    /// `users` is (user_id, display_name, @username), answering the search
    /// for `term`.
    pub fn set_candidates(&self, term: &str, users: Vec<(String, String, String)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        if list.read(cx).term(1, cx) != term.trim() {
            return; // Answered a question nobody is asking any more.
        }
        let rows = users
            .into_iter()
            .map(|(id, display_name, username)| {
                let on_add = self.on_add.clone();
                Row::new(id.clone(), display_name)
                    .subtitle(username)
                    .button("Add", "Added", move |cx| on_add(id.clone(), cx))
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(1, rows, cx));
    }
}
