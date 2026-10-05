use std::cell::RefCell;

use gpui_kit::App;

use super::caption_line::caption_line;
use crate::state::SharedState;

pub struct CallDock {
    /// One line of live transcription, or a reaction; empty when there is
    /// neither. A subtitle, not a transcript: it is replaced as it goes.
    pub(super) caption: RefCell<String>,
}

impl CallDock {
    pub fn new() -> Self {
        CallDock {
            caption: RefCell::new(String::new()),
        }
    }

    /// Shows one line of live transcription, or clears it.
    pub fn set_caption(&self, who: &str, text: &str, cx: &mut App) {
        *self.caption.borrow_mut() = caption_line(who, text);
        // The dock is under the channel list, or under the conversation in a
        // narrow window; speech arrives a few words at a time.
        crate::ui::redraw(&[crate::ui::Part::Sidebar, crate::ui::Part::Chat], cx);
    }

    /// The dock is drawn from the call in the state, so a refresh is a frame.
    /// Answers whether there is a call to show at all.
    pub fn refresh(&self, state: &SharedState, cx: &mut App) -> bool {
        cx.refresh_windows();
        let in_call = state.borrow().call.is_some();
        if !in_call {
            // A caption belongs to the call it was said in.
            self.caption.borrow_mut().clear();
        }
        in_call
    }
}
