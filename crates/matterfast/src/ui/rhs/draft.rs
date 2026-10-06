use std::rc::Rc;

use gpui_kit::App;

use super::right_panel::RightPanel;
use crate::ui::autocomplete::{Candidate, Composer};
use crate::ui::{Action, Ui};

impl RightPanel {
    pub fn focus_composer(&self, cx: &mut App) {
        let Some(composer) = self.composer.borrow().clone() else {
            return;
        };
        self.window.update(cx, move |window, cx| {
            composer.update(cx, |composer, cx| composer.focus(window, cx));
        });
    }

    /// What is in the thread's reply box.
    pub fn composer_text(&self, cx: &App) -> String {
        self.composer
            .borrow()
            .as_ref()
            .map(|composer| composer.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// Puts a thread draft back. Guarded like the channel composer's: setting
    /// the text fires a change, which must not be read as typing.
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
                composer.set_selected_range(text.len()..text.len(), cx);
            });
        });
        self.restoring.set(false);
        self.completions.borrow_mut().close();
    }

    /// The reply box changed; says so unless it was us putting a draft back,
    /// and works out what, if anything, is being completed in it.
    pub(crate) fn composer_changed(&self, ui: &Rc<Ui>, cx: &mut App) {
        if self.restoring.get() {
            return;
        }
        if let Some(composer) = self.composer.borrow().clone() {
            crate::ui::session::completion_asked(ui, Composer::Thread, &composer, cx);
        }
        ui.dispatch(Action::ThreadDraftChanged, cx);
    }

    /// Enter in the reply box: picks the selected candidate while the list
    /// is up, and sends otherwise.
    pub(super) fn composer_submitted(&self, ui: &Rc<Ui>, cx: &mut App) {
        if self.accept_completion(ui, cx) {
            return;
        }
        self.submit(ui, cx);
    }

    /// Replaces the token under the cursor with the selected candidate.
    /// `false` when the list is not open.
    pub(crate) fn accept_completion(&self, ui: &Rc<Ui>, cx: &mut App) -> bool {
        if !crate::ui::session::accept_in(&self.completions, &self.composer, &self.window, cx) {
            return false;
        }
        // The text did change, and the draft should follow it.
        ui.dispatch(Action::ThreadDraftChanged, cx);
        crate::ui::refresh(cx);
        true
    }

    /// Answers the reply box's outstanding completion query.
    pub(crate) fn set_completions(&self, items: Vec<Candidate>, cx: &mut App) {
        self.completions.borrow_mut().set(items);
        crate::ui::refresh(cx);
    }

    /// Whether the completion list is up, and so has first refusal on keys.
    pub(crate) fn completing(&self) -> bool {
        self.completions.borrow().is_open()
    }

    pub(super) fn submit(&self, ui: &Rc<Ui>, cx: &mut App) {
        let text = self.composer_text(cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        self.set_composer_text("", cx);
        ui.dispatch(Action::ReplyInThread(text), cx);
    }
}
