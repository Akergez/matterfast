use std::rc::Rc;

use gpui_kit::{App, Entity};

use super::list::{open_list, List, Row};
use crate::ui::kit::Lucide;
use crate::ui::Ui;

/// Browse and join channels.
///
/// The dialog outlives this call, so the handle keeps the list around: the
/// caller searches the server on `on_search` and pours the answer back in
/// through [`Self::set_results`].
pub struct ChannelBrowser {
    list: Option<Entity<List>>,
    on_join: Rc<dyn Fn(String, &mut App)>,
}

impl ChannelBrowser {
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        on_search: impl Fn(String, &mut App) + 'static,
        on_join: impl Fn(String, &mut App) + 'static,
    ) -> Self {
        let on_search = Rc::new(on_search);
        let typed = on_search.clone();
        let list = open_list(ui, cx, "Browse channels", move |list, window, cx| {
            let section = list.section(
                "",
                Some((Lucide::Hash, "No channels found", "Try a different name.")),
            );
            list.searchable(
                section,
                "Search channels",
                move |term, cx| typed(term, cx),
                window,
                cx,
            );
        });
        // Ask once on open: a browser that is empty until you type is a search
        // box, and browsing is the point.
        cx.defer(move |cx| on_search(String::new(), cx));
        ChannelBrowser {
            list,
            on_join: Rc::new(on_join),
        }
    }

    /// `channels` is (id, display_name, purpose, already_member), answering
    /// the search for `term`.
    pub fn set_results(
        &self,
        term: &str,
        channels: Vec<(String, String, String, bool)>,
        cx: &mut App,
    ) {
        let Some(list) = &self.list else { return };
        // A search is a round trip, and a narrow term can come back before
        // the broad one typed before it — without this, the broad answer
        // lands last and replaces the filtered list.
        if list.read(cx).term(0, cx) != term.trim() {
            return; // Answered a question nobody is asking any more.
        }
        let rows = channels
            .into_iter()
            .map(|(id, display_name, purpose, already_member)| {
                let row = Row::new(id.clone(), display_name).subtitle(purpose);
                if already_member {
                    row.label("Joined")
                } else {
                    // The dialog stays open so you can join several at once.
                    let on_join = self.on_join.clone();
                    row.button("Join", "Joined", move |cx| on_join(id.clone(), cx))
                }
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }
}
