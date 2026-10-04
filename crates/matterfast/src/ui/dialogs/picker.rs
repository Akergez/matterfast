use std::rc::Rc;

use gpui_kit::component::WindowExt;
use gpui_kit::{App, Entity};

use super::list::{open_list, List, Row};
use crate::ui::kit::Lucide;
use crate::ui::Ui;

/// Pick one thing out of a directory too large to list: people, channels.
/// The caller answers each search through [`Self::set_results`].
pub struct Picker {
    list: Option<Entity<List>>,
    ui: Rc<Ui>,
    on_pick: Rc<dyn Fn(String, &mut App)>,
}

impl Picker {
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        title: &str,
        placeholder: &'static str,
        on_search: impl Fn(String, &mut App) + 'static,
        on_pick: impl Fn(String, &mut App) + 'static,
    ) -> Self {
        let on_search = Rc::new(on_search);
        let typed = on_search.clone();
        let list = open_list(ui, cx, title, move |list, window, cx| {
            let section = list.section(
                "",
                Some((Lucide::Search, "Nothing found", "Try a different name.")),
            );
            list.searchable(section, placeholder, move |term, cx| typed(term, cx), window, cx);
        });
        // Typing is the whole point of this dialog, so the box has the focus
        // from the start — after the dialog has taken it for itself.
        if let Some(input) = list
            .as_ref()
            .and_then(|list| list.read(cx).sections.first()?.search.clone())
        {
            let ui = ui.clone();
            cx.defer(move |cx| {
                ui.with_window(cx, |window, cx| {
                    input.update(cx, |input, cx| input.focus(window, cx))
                });
            });
        }
        // Something to choose from before a letter is typed.
        cx.defer(move |cx| on_search(String::new(), cx));
        Picker {
            list,
            ui: ui.clone(),
            on_pick: Rc::new(on_pick),
        }
    }

    /// `entries` is (id, name, subtitle), answering the search for `term`.
    /// Picking one answers with its id and puts the dialog away.
    pub fn set_results(&self, term: &str, entries: Vec<(String, String, String)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        // The same guard as the channel browser: an answer to a question
        // nobody is asking any more must not replace the current one.
        if list.read(cx).term(0, cx) != term.trim() {
            return;
        }
        let rows = entries
            .into_iter()
            .map(|(id, name, subtitle)| {
                let (ui, on_pick) = (self.ui.clone(), self.on_pick.clone());
                Row::new(id.clone(), name)
                    .subtitle(subtitle)
                    .on_activate(move |cx| {
                        on_pick(id.clone(), cx);
                        ui.with_window(cx, |window, cx| window.close_dialog(cx));
                    })
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }
}
