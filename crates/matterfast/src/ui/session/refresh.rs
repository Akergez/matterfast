//! Redrawing parts of the window after the state changed.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::ui::autocomplete;

impl Ui {
    /// Answers an outstanding completion query, and remembers the answer so a
    /// picture landing later can redraw the popover without re-querying it.
    pub(crate) fn set_completions(&self, items: Vec<autocomplete::Candidate>, cx: &mut App) {
        *self.last_completions.borrow_mut() = items.clone();
        self.chat.set_completions(items, cx);
    }

    /// A picture that finished downloading after the popover opened belongs
    /// to one of the rows already drawn — redraw it with whatever pictures
    /// are available now. No query, no network: same candidates, same order.
    pub(crate) fn refresh_completion_avatars(&self, cx: &mut App) {
        let mut items = self.last_completions.borrow().clone();
        if items.is_empty() {
            return;
        }
        let mut changed = false;
        for item in &mut items {
            if item.image.is_some() {
                continue;
            }
            if let Some(id) = &item.user_id {
                if let Some(texture) = self.avatars.texture(id) {
                    item.image = Some(texture);
                    changed = true;
                }
            }
        }
        if changed {
            self.set_completions(items, cx);
        }
    }

    /// Redraws only the message surfaces — used when something cosmetic lands,
    /// such as an avatar finishing its download.
    pub(crate) fn refresh_messages(self: &Rc<Self>, cx: &mut App) {
        self.chat.refresh(&self.state, cx);
        self.right.refresh(&self.state, cx);
    }

    /// Redraws only the thread panel — used when a thread's own data changes,
    /// which has nothing to do with the channel feed behind it. The feed's
    /// "N replies" footer is kept live separately, by the ordinary post-apply
    /// path that already runs on every incoming reply.
    pub(crate) fn refresh_thread_panel(self: &Rc<Self>, cx: &mut App) {
        self.right.refresh(&self.state, cx);
    }

    pub(crate) fn refresh_all(self: &Rc<Self>, cx: &mut App) {
        self.channels.refresh(cx);
        self.refresh_messages(cx);
        self.refresh_call_ui(cx);
        self.refresh_title(cx);
        self.hydrate_dm_teammates(cx);
    }
}
