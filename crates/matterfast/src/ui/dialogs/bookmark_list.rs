use std::rc::Rc;

use gpui_kit::{App, AppContext, Entity};

use super::forms::{Field, Form};
use super::list::{open_list, List, Row};
use crate::ui::kit::Lucide;
use crate::ui::Ui;

/// A channel's bookmarks, filled in by [`Self::set_bookmarks`].
///
/// Adding one is a form in the same dialog rather than a second one: it is
/// two fields, and you usually add several in a row.
pub struct BookmarkList {
    list: Option<Entity<List>>,
    on_open: Rc<dyn Fn(String, &mut App)>,
    on_delete: Rc<dyn Fn(String, &mut App)>,
}

impl BookmarkList {
    pub fn present(
        ui: &Rc<Ui>,
        cx: &mut App,
        on_add: impl Fn(String, String, &mut App) + 'static,
        on_open: impl Fn(String, &mut App) + 'static,
        on_delete: impl Fn(String, &mut App) + 'static,
    ) -> Self {
        let list = open_list(ui, cx, "Bookmarks", move |list, window, cx| {
            list.section(
                "Bookmarks",
                Some((Lucide::Bookmark, "No bookmarks yet", "")),
            );
            list.form_title = "Add a bookmark".to_string();
            list.form = Some(cx.new(|cx| {
                Form::new(
                    vec![Field::text("Name (optional)", ""), Field::text("Link", "")],
                    window,
                    cx,
                )
            }));
            list.form_action = Some(Rc::new(move |values, cx| {
                let name = values[0].text().trim().to_string();
                let link = values[1].text().trim().to_string();
                // A bookmark without a link is nothing to save; the name can
                // be filled in from the link by whoever handles this.
                if !link.is_empty() {
                    on_add(name, link, cx);
                }
            }));
        });
        BookmarkList {
            list,
            on_open: Rc::new(on_open),
            on_delete: Rc::new(on_delete),
        }
    }

    /// `bookmarks` is (id, display_name, link_url).
    pub fn set_bookmarks(&self, bookmarks: Vec<(String, String, String)>, cx: &mut App) {
        let Some(list) = &self.list else { return };
        let rows = bookmarks
            .into_iter()
            .map(|(id, display_name, link_url)| {
                let on_open = self.on_open.clone();
                let on_delete = self.on_delete.clone();
                let link = link_url.clone();
                // The whole row opens it, which is what a bookmark is for; the
                // button beside it is the only other thing you can do.
                Row::new(id.clone(), display_name)
                    .subtitle(link_url)
                    .on_activate(move |cx| on_open(link.clone(), cx))
                    .icon_button(Lucide::Trash, "Remove bookmark", move |cx| {
                        on_delete(id.clone(), cx)
                    })
            })
            .collect();
        list.update(cx, |list, cx| list.set_rows(0, rows, cx));
    }
}
