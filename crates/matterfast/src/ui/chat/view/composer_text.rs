use std::rc::Rc;

use gpui_kit::App;

use super::chat_view::ChatView;
use crate::ui::{autocomplete, Action, Ui};

impl ChatView {
    /// What is in the composer right now.
    pub fn composer_text(&self, cx: &App) -> String {
        self.composer
            .borrow()
            .as_ref()
            .map(|composer| composer.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// Puts a draft back. Setting the text fires a change event, which would
    /// otherwise be read as the user typing and save the draft straight back —
    /// hence the guard.
    pub fn set_composer_text(&self, text: &str, cx: &mut App) {
        if self.composer_text(cx) == text {
            return;
        }
        let Some(composer) = self.composer.borrow().clone() else {
            return;
        };
        self.restoring.set(true);
        let text = text.to_string();
        self.window.update(cx, move |window, cx| {
            composer.update(cx, |composer, cx| {
                composer.set_value(text.clone(), window, cx);
                // Put the cursor where they left off, not at the front.
                composer.set_selected_range(text.len()..text.len(), cx);
            });
        });
        self.restoring.set(false);
        self.completions.borrow_mut().close();
    }

    pub fn focus_composer(&self, cx: &mut App) {
        let Some(composer) = self.composer.borrow().clone() else {
            return;
        };
        self.window.update(cx, move |window, cx| {
            composer.update(cx, |composer, cx| composer.focus(window, cx));
        });
    }

    /// The composer's text changed. Works out what, if anything, is being
    /// completed, and says so.
    pub(crate) fn composer_changed(&self, ui: &Rc<Ui>, cx: &mut App) {
        if self.restoring.get() {
            return;
        }
        let Some(composer) = self.composer.borrow().clone() else {
            return;
        };
        let (text, cursor) = {
            let composer = composer.read(cx);
            (composer.value().to_string(), composer.cursor())
        };
        let query = autocomplete::token_at(&text, cursor.min(text.len()));
        let changed = self.completions.borrow().query != query;
        if changed {
            {
                let mut completions = self.completions.borrow_mut();
                if query.is_none() {
                    completions.close();
                }
                completions.query = query.clone();
            }
            ui.dispatch(Action::Complete(query), cx);
        }
        // Restoring a draft is not typing, and was returned from above.
        ui.dispatch(Action::ComposerChanged(!text.is_empty()), cx);
    }
}
