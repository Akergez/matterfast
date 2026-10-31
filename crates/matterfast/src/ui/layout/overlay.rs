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
    /// Whether the system's gesture of back is carrying the panel off.
    held: Cell<bool>,
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

    /// Lays the panel out by the system's gesture of back while one is under
    /// way, `progress` from 0 to 1 where the panel is gone. Once the gesture
    /// is over (`None`) a panel still shown goes back where it was, and one
    /// that back closed settles off from where the gesture left it.
    pub(crate) fn follow_back(&self, progress: Option<f32>) {
        match progress {
            Some(progress) if self.shown() => {
                self.held.set(true);
                self.visible.set(1.0 - progress.clamp(0.0, 1.0));
                self.ticked.set(None);
            }
            _ => {
                if self.held.replace(false) && self.shown() {
                    self.visible.set(1.0);
                }
            }
        }
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

    #[test]
    fn the_gesture_of_back_carries_a_shown_panel_and_puts_it_back_if_cancelled() {
        let overlay = Overlay::default();
        overlay.shown.set(true);
        overlay.visible.set(1.0);
        overlay.follow_back(Some(0.25));
        assert_eq!(overlay.visible(), 0.75);
        // Cancelled: still shown, so back where it was.
        overlay.follow_back(None);
        assert_eq!(overlay.visible(), 1.0);

        // Done: back closed it, and it goes off from where it was left.
        overlay.follow_back(Some(0.9));
        overlay.shown.set(false);
        overlay.follow_back(None);
        assert!((overlay.visible() - 0.1).abs() < 1e-6);
    }
}
