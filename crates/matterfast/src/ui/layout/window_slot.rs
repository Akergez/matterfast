use std::cell::Cell;

use gpui_kit::{AnyWindowHandle, App, Window};

/// The window a session is shown in, when it is shown in one.
///
/// The session outlives its window, so everything that needs a window — a
/// toast, the composer's text, the title — goes through here and quietly does
/// nothing while there is none.
#[derive(Default)]
pub struct WindowSlot(Cell<Option<AnyWindowHandle>>);

impl WindowSlot {
    pub fn set(&self, window: Option<AnyWindowHandle>) {
        self.0.set(window);
    }

    /// Runs `f` with the window, if there is one.
    ///
    /// Session code only ever runs between window updates — from a finished
    /// request, a timer, or a queued action — never inside one, which is what
    /// makes asking for the window here sound. A caller that breaks that rule
    /// finds the window already lent out; that is logged rather than hidden,
    /// because the symptom would otherwise be a click that does nothing.
    pub fn update<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        let handle = self.0.get()?;
        match handle.update(cx, |_, window, cx| f(window, cx)) {
            Ok(result) => Some(result),
            Err(error) => {
                if cx.windows().contains(&handle) {
                    tracing::error!(%error, "the window was asked for while it was busy");
                } else {
                    // Closed since we last looked.
                    self.0.set(None);
                }
                None
            }
        }
    }
}
