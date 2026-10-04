use std::cell::Cell;

use gpui_kit::App;

/// Whether the right-hand panel is showing, and how.
#[derive(Default)]
pub struct Overlay {
    shown: Cell<bool>,
    /// Laid over the conversation rather than beside it.
    collapsed: Cell<bool>,
}

impl Overlay {
    pub fn set_show_sidebar(&self, shown: bool, cx: &mut App) {
        self.shown.set(shown);
        cx.refresh_windows();
    }

    pub fn set_collapsed(&self, collapsed: bool, cx: &mut App) {
        if self.collapsed.replace(collapsed) != collapsed {
            cx.refresh_windows();
        }
    }

    pub fn shown(&self) -> bool {
        self.shown.get()
    }
}
