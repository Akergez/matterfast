use std::cell::Cell;

use gpui_kit::{App, Window};

/// Whether the right-hand panel is showing, and how.
#[derive(Default)]
pub struct Overlay {
    shown: Cell<bool>,
    /// Laid over the conversation rather than beside it.
    collapsed: Cell<bool>,
    visible: Cell<f32>,
    ticked: Cell<Option<std::time::Instant>>,
}

impl Overlay {
    pub fn set_show_sidebar(&self, shown: bool, cx: &mut App) {
        self.shown.set(shown);
        if shown {
            self.visible.set(1.0);
        }
        self.ticked.set(None);
        crate::ui::refresh(cx);
    }

    pub fn set_collapsed(&self, collapsed: bool, cx: &mut App) {
        if self.collapsed.replace(collapsed) != collapsed {
            crate::ui::refresh(cx);
        }
    }

    pub fn shown(&self) -> bool {
        self.shown.get()
    }

    pub(crate) fn visible(&self) -> f32 {
        self.visible.get()
    }

    /// The same settling curve as the conversation page, retaining the panel
    /// until it has travelled beyond the right edge.
    pub(crate) fn advance(&self, window: &mut Window) -> f32 {
        if self.shown() || self.visible.get() == 0.0 {
            self.ticked.set(None);
            return self.visible.get();
        }
        let now = std::time::Instant::now();
        let elapsed = self
            .ticked
            .replace(Some(now))
            .map_or(1.0 / 60.0, |last| (now - last).as_secs_f32())
            .min(0.05);
        let visible = settle(self.visible.get(), elapsed);
        self.visible.set(visible);
        window.request_animation_frame();
        visible
    }
}

fn settle(visible: f32, elapsed: f32) -> f32 {
    let next = visible * (-elapsed / 0.06).exp();
    if next < 0.004 {
        0.0
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_keeps_the_page_visible_until_it_settles_offscreen() {
        let mut visible = settle(1.0, 1.0 / 60.0);
        assert!(visible > 0.0 && visible < 1.0);
        for _ in 0..30 {
            let next = settle(visible, 1.0 / 60.0);
            assert!(next <= visible);
            visible = next;
        }
        assert_eq!(visible, 0.0);
    }
}
