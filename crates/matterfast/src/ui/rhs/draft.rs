use std::rc::Rc;

use gpui_kit::App;

use super::right_panel::RightPanel;
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
    }

    /// The reply box changed; says so unless it was us putting a draft back.
    pub(super) fn composer_changed(&self, ui: &Rc<Ui>, cx: &mut App) {
        if !self.restoring.get() {
            ui.dispatch(Action::ThreadDraftChanged, cx);
        }
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
