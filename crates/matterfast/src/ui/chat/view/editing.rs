use gpui_kit::App;

use super::chat_view::ChatView;

impl ChatView {
    /// Puts the composer into edit mode for an existing post.
    ///
    /// Editing reuses the composer rather than opening a second one: there is
    /// only ever one message being written at a time, and a separate box would
    /// be a second place to lose text in.
    pub fn begin_edit(&self, post_id: &str, text: &str, cx: &mut App) {
        *self.editing.borrow_mut() = Some(post_id.to_string());
        self.set_composer_text(text, cx);
        self.focus_composer(cx);
    }

    /// Leaves edit mode, clearing the composer.
    pub fn end_edit(&self, cx: &mut App) {
        *self.editing.borrow_mut() = None;
        self.set_composer_text("", cx);
    }

    /// The post being edited, if any.
    pub fn editing(&self, _cx: &App) -> Option<String> {
        self.editing.borrow().clone()
    }

    /// The priority for the message being written: "", "important" or
    /// "urgent". Empty means standard, which is what the server expects.
    pub fn priority(&self, _cx: &App) -> String {
        self.priority.borrow().clone()
    }

    /// Back to standard once a message has gone out. Priority is per message,
    /// and a sticky "urgent" would quietly escalate everything after it.
    pub fn reset_priority(&self, cx: &mut App) {
        self.priority.borrow_mut().clear();
        cx.refresh_windows();
    }

    pub(crate) fn set_priority(&self, priority: &str, cx: &mut App) {
        *self.priority.borrow_mut() = match priority {
            "important" | "urgent" => priority.to_string(),
            _ => String::new(),
        };
        cx.refresh_windows();
    }
}
