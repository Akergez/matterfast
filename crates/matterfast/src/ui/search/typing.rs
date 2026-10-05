use std::rc::Rc;

use gpui_kit::App;

use super::accept::accept;
use super::constants::ROWS;
use super::hint::Hint;
use super::hint_at::hint_at;
use super::search_box::SearchBox;
use super::suggestion::Suggestion;
use super::suggestions::suggestions;
use crate::ui::{Action, Ui};

impl SearchBox {
    /// What `from:` is being completed with, if that is what is being typed:
    /// the session looks the same name up on the server.
    pub(crate) fn asking_for_person(&self) -> Option<String> {
        match &*self.hint.borrow() {
            Some(Hint::From(typed)) => Some(typed.clone()),
            _ => None,
        }
    }

    /// People the server found for the name being typed, added under the
    /// ones already listed.
    pub(crate) fn add_people(&self, found: Vec<Suggestion>, cx: &mut App) {
        let mut rows = self.rows.borrow_mut();
        for row in found {
            if rows.len() >= ROWS {
                break;
            }
            if !rows.iter().any(|known| known.insert == row.insert) {
                rows.push(row);
            }
        }
        drop(rows);
        crate::ui::refresh(cx);
    }

    /// The text or the cursor moved: works out what to suggest now.
    pub(super) fn changed(&self, ui: &Rc<Ui>, cx: &mut App) {
        let Some(input) = self.input.borrow().clone() else {
            return;
        };
        let (text, cursor) = {
            let input = input.read(cx);
            (input.value().to_string(), input.cursor())
        };
        let hint = hint_at(&text, cursor.min(text.len()));
        let rows = hint.as_ref().map_or_else(Vec::new, |hint| {
            let today = chrono::Local::now().date_naive();
            suggestions(&ui.state.borrow(), hint, today)
        });
        let person = matches!(hint, Some(Hint::From(_)));
        *self.hint.borrow_mut() = hint;
        *self.rows.borrow_mut() = rows;
        self.selected.set(None);
        if person {
            ui.dispatch(Action::SearchPeople, cx);
        }
        crate::ui::refresh(cx);
    }

    pub(super) fn step(&self, delta: i32) {
        let len = self.rows.borrow().len();
        if len == 0 {
            return;
        }
        let next = match self.selected.get() {
            None if delta < 0 => len - 1,
            None => 0,
            Some(at) => crate::ui::autocomplete::next_index(at, delta, len),
        };
        self.selected.set(Some(next));
    }

    /// Writes the row at `index` into the line and goes on suggesting from
    /// where that leaves the cursor.
    pub(super) fn pick(&self, index: usize, ui: &Rc<Ui>, cx: &mut App) {
        let Some(picked) = self.rows.borrow().get(index).cloned() else {
            return;
        };
        let Some(input) = self.input.borrow().clone() else {
            return;
        };
        let (text, cursor) = {
            let input = input.read(cx);
            (input.value().to_string(), input.cursor())
        };
        let (text, caret) = accept(&text, cursor, &picked);
        self.window.update(cx, move |window, cx| {
            input.update(cx, |input, cx| {
                input.set_value(text.clone(), window, cx);
                input.set_selected_range(caret..caret, cx);
                input.focus(window, cx);
            });
        });
        self.changed(ui, cx);
    }

    /// Enter: searches for what is in the box.
    pub(super) fn submit(&self, ui: &Rc<Ui>, cx: &mut App) {
        let Some(input) = self.input.borrow().clone() else {
            return;
        };
        let terms = input.read(cx).value().trim().to_string();
        if terms.is_empty() {
            return;
        }
        self.close();
        ui.dispatch(Action::Search(terms), cx);
        crate::ui::refresh(cx);
    }
}
