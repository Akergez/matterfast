use std::cell::Cell;

use gpui_kit::{App, Window};

use super::swipe::Swipe;

/// The channel list and the conversation: side by side when there is room,
/// two pages of a stack when there is not.
pub struct Split {
    /// When collapsed, which page is in front. A chat app should land on the
    /// conversation, not the sidebar.
    show_content: Cell<bool>,
    pub(crate) collapsed: Cell<bool>,
    /// How far the channel list has slid in over the conversation, from 0 to
    /// 1. It trails `show_content` while it animates, and follows the finger
    /// while one is dragging it.
    open: Cell<f32>,
    /// When `open` last moved by itself, which is what its next step is
    /// measured from.
    ticked: Cell<Option<std::time::Instant>>,
    swipe: Cell<Swipe>,
    /// The last sideways movement of the finger, which says where it was
    /// heading when it let go.
    heading: Cell<f32>,
}

impl Default for Split {
    fn default() -> Self {
        Split {
            show_content: Cell::new(true),
            collapsed: Cell::new(false),
            open: Cell::new(0.0),
            ticked: Cell::new(None),
            swipe: Cell::new(Swipe::Idle),
            heading: Cell::new(0.0),
        }
    }
}

impl Split {
    /// How far in the channel list is for this frame, moving it a step
    /// towards where it belongs. Asks for another frame until it is there.
    pub fn advance(&self, window: &mut Window) -> f32 {
        let target = if self.show_content.get() { 0.0 } else { 1.0 };
        let mut open = self.open.get();
        if self.swipe.get() == Swipe::Dragging {
            return open;
        }
        if open == target {
            self.ticked.set(None);
            return open;
        }
        let now = std::time::Instant::now();
        let elapsed = self
            .ticked
            .replace(Some(now))
            .map_or(1.0 / 60.0, |last| (now - last).as_secs_f32())
            .min(0.05);
        // Fast at first and slowing into place, from wherever it is — which
        // is what lets it carry on from the point a finger let go at.
        open += (target - open) * (1.0 - (-elapsed / 0.06).exp());
        if (target - open).abs() < 0.004 {
            open = target;
        }
        self.open.set(open);
        window.request_animation_frame();
        open
    }

    /// One step of a pan across a collapsed window. Says whether the step was
    /// taken — it moved the channel list, and is not a scroll for anything
    /// underneath. `width` is how wide the channel list is.
    pub fn swiped(&self, dx: f32, dy: f32, phase: gpui_kit::TouchPhase, width: f32) -> bool {
        use gpui_kit::TouchPhase;
        if phase == TouchPhase::Started {
            // A pan is locked to one axis from its first step. Sideways, it
            // is ours if there is somewhere for the list to go that way.
            let open = self.open.get();
            let ours = dy == 0.0 && ((dx > 0.0 && open < 1.0) || (dx < 0.0 && open > 0.0));
            self.swipe.set(if ours { Swipe::Dragging } else { Swipe::Idle });
            self.heading.set(0.0);
        }
        match self.swipe.get() {
            Swipe::Idle => false,
            Swipe::Coasting => {
                if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    self.swipe.set(Swipe::Idle);
                }
                true
            }
            Swipe::Dragging => {
                let open = (self.open.get() + dx / width).clamp(0.0, 1.0);
                self.open.set(open);
                if dx != 0.0 {
                    self.heading.set(dx);
                }
                if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    // A flick goes where it was heading; a list that was
                    // carried and set down goes to whichever end is nearer.
                    let heading = self.heading.get();
                    let show_list = if heading.abs() > 1.5 { heading > 0.0 } else { open > 0.5 };
                    self.show_content.set(!show_list);
                    self.ticked.set(None);
                    self.swipe.set(Swipe::Coasting);
                }
                true
            }
        }
    }

    pub fn set_show_content(&self, show: bool, cx: &mut App) {
        if self.show_content.replace(show) != show {
            cx.refresh_windows();
        }
    }
}
