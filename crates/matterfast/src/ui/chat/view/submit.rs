use std::rc::Rc;

use gpui_kit::App;

use super::chat_view::ChatView;
use crate::ui::{autocomplete, Action, Ui};

impl ChatView {
    /// Enter in the composer: picks the selected candidate while the list is
    /// up, and sends otherwise.
    pub(crate) fn composer_submitted(&self, ui: &Rc<Ui>, cx: &mut App) {
        if self.accept_completion(ui, cx) {
            return;
        }
        self.submit(ui, cx);
    }

    pub(crate) fn submit(&self, ui: &Rc<Ui>, cx: &mut App) {
        let text = self.composer_text(cx).trim().to_string();
        // An empty message with files waiting is still a message; the session
        // decides, since it is what knows about the files.
        if text.is_empty() && !ui.has_pending_files() {
            return;
        }
        self.set_composer_text("", cx);
        ui.dispatch(Action::Send(text), cx);
    }

    /// Replaces the token under the cursor with the selected candidate.
    /// `false` when the list is not open, so the key means what it usually
    /// does.
    pub(crate) fn accept_completion(&self, ui: &Rc<Ui>, cx: &mut App) -> bool {
        let Some(insert) = self
            .completions
            .borrow()
            .is_open()
            .then(|| self.completions.borrow().chosen())
            .flatten()
        else {
            return false;
        };
        let Some(composer) = self.composer.borrow().clone() else {
            return false;
        };
        let (text, cursor) = {
            let composer = composer.read(cx);
            (composer.value().to_string(), composer.cursor())
        };
        let (text, caret) = autocomplete::accept(&text, cursor, &insert);
        self.completions.borrow_mut().close();
        self.window.update(cx, move |window, cx| {
            composer.update(cx, |composer, cx| {
                composer.set_value(text.clone(), window, cx);
                composer.set_selected_range(caret..caret, cx);
            });
        });
        // The text did change, and the draft should follow it.
        ui.dispatch(Action::ComposerChanged(true), cx);
        cx.refresh_windows();
        true
    }

    /// Whether the completion list is up, and so has first refusal on keys.
    pub(crate) fn completing(&self) -> bool {
        self.completions.borrow().is_open()
    }
}
