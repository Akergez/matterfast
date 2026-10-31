use std::cell::Cell;

use gpui_kit::{App, Window};

use super::swipe::Swipe;

/// The channel list and the conversation: side by side when there is room,
/// two pages of a stack when there is not.
///
/// On a phone the list is the main screen, as it is in Telegram: the window
/// opens on it, a conversation is a page laid over it, and going back takes
/// that page away again. It is not a drawer beside the conversation.
pub struct Split {
    /// When collapsed, which page is in front. The window lands on the list:
    /// what to read is chosen there.
    show_content: Cell<bool>,
    pub(crate) collapsed: Cell<bool>,
    /// How much of the window the channel list has to itself, from 0 — the
    /// conversation covers it — to 1. It trails `show_content` while it
    /// animates, and follows the finger while one is carrying the
    /// conversation off.
    open: Cell<f32>,
    /// When `open` last moved by itself, which is what its next step is
    /// measured from.
    ticked: Cell<Option<std::time::Instant>>,
    swipe: Cell<Swipe>,
    /// Whether the system's gesture of back is carrying the conversation off:
    /// `open` is where the gesture has it, not where it is going.
    held: Cell<bool>,
    /// The last sideways movement of the finger, which says where it was
    /// heading when it let go, and when it made it: a finger that stopped
    /// before it let go was heading nowhere.
    heading: Cell<(f32, Option<std::time::Instant>)>,
}

/// How long a finger may rest before it lets go and still have flicked.
const FLICK_PAUSE: std::time::Duration = std::time::Duration::from_millis(100);

impl Default for Split {
    fn default() -> Self {
        Split {
            show_content: Cell::new(false),
            collapsed: Cell::new(false),
            open: Cell::new(1.0),
            ticked: Cell::new(None),
            swipe: Cell::new(Swipe::Idle),
            held: Cell::new(false),
            heading: Cell::new((0.0, None)),
        }
    }
}

impl Split {
    /// How far in the channel list is for this frame, moving it a step
    /// towards where it belongs. Asks for another frame until it is there.
    pub fn advance(&self, window: &mut Window) -> f32 {
        let target = if self.show_content.get() { 0.0 } else { 1.0 };
        let mut open = self.open.get();
        if self.swipe.get() == Swipe::Dragging || self.held.get() {
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

    /// How much of the window the channel list has to itself, from 0 to 1.
    pub(crate) fn open(&self) -> f32 {
        self.open.get()
    }

    /// Whether the conversation is the page in front, or on its way there.
    pub fn showing_content(&self) -> bool {
        self.show_content.get()
    }

    /// Whether the list has the window to itself: no conversation over it,
    /// and none sliding on or off.
    pub fn list_in_front(&self) -> bool {
        self.open.get() == 1.0 && !self.show_content.get()
    }

    /// One step of a pan across a collapsed window. Says whether the step was
    /// taken — it carried the conversation, and is not a scroll for anything
    /// underneath. `width` is how wide the window is.
    pub fn swiped(&self, dx: f32, dy: f32, phase: gpui_kit::TouchPhase, width: f32) -> bool {
        use gpui_kit::TouchPhase;
        if phase == TouchPhase::Started {
            // A pan is locked to one axis from its first step. To the right
            // over a conversation, it is ours: it is the way back to the
            // list. There is no pan that opens a conversation — a press on
            // its row does — and sideways over the list is for its tabs.
            let ours = dy == 0.0 && dx > 0.0 && self.open.get() < 1.0;
            self.swipe.set(if ours { Swipe::Dragging } else { Swipe::Idle });
            self.heading.set((0.0, None));
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
                    self.heading.set((dx, Some(std::time::Instant::now())));
                }
                if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    // A flick goes where it was heading; a page that was
                    // carried and set down goes to whichever end is nearer.
                    let heading = match self.heading.get() {
                        (dx, Some(at)) if at.elapsed() < FLICK_PAUSE => dx,
                        _ => 0.0,
                    };
                    let show_list = if heading.abs() > 1.5 { heading > 0.0 } else { open > 0.5 };
                    self.show_content.set(!show_list);
                    self.ticked.set(None);
                    self.swipe.set(Swipe::Coasting);
                }
                true
            }
        }
    }

    /// Lays the conversation out by the system's gesture of back while one
    /// is under way, `progress` from 0 where it started to 1 where the page
    /// is gone, so that the list shows from under it. The page goes off to
    /// the right, as it does under a finger: it is the way it came in.
    /// Once the gesture is over (`None`) the page settles from where it was
    /// left, on the list if back closed the conversation.
    pub fn follow_back(&self, progress: Option<f32>) {
        match progress {
            Some(progress) => {
                self.held.set(true);
                self.open.set(progress.clamp(0.0, 1.0));
                self.ticked.set(None);
            }
            None => {
                if self.held.replace(false) {
                    self.ticked.set(None);
                }
            }
        }
    }

    pub fn set_show_content(&self, show: bool, cx: &mut App) {
        if self.show_content.replace(show) != show {
            crate::ui::refresh(cx);
        }
    }

    /// A dismissed full-screen panel reveals the list beneath its own slide.
    pub(crate) fn reveal_list(&self, cx: &mut App) {
        self.show_content.set(false);
        self.open.set(1.0);
        self.ticked.set(None);
        self.swipe.set(Swipe::Idle);
        crate::ui::refresh(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TouchPhase;

    /// A split on a phone with a conversation in front.
    fn conversation() -> Split {
        let split = Split::default();
        split.collapsed.set(true);
        split.show_content.set(true);
        split.open.set(0.0);
        split
    }

    #[test]
    fn the_gesture_of_back_carries_the_conversation_and_lets_it_settle() {
        let split = conversation();
        split.follow_back(Some(0.3));
        assert_eq!(split.open(), 0.3);
        split.follow_back(Some(1.4));
        assert_eq!(split.open(), 1.0);
        // Over: where it was left is where it settles from.
        split.follow_back(None);
        assert!(!split.held.get());
        assert_eq!(split.open(), 1.0);
    }

    #[test]
    fn a_flick_goes_where_it_was_heading_and_a_rest_where_it_is_nearer() {
        let split = conversation();
        split.swiped(40.0, 0.0, TouchPhase::Started, 400.0);
        split.swiped(40.0, 0.0, TouchPhase::Moved, 400.0);
        split.swiped(0.0, 0.0, TouchPhase::Ended, 400.0);
        assert!(!split.showing_content(), "flicked to the right, back to the list");

        let split = conversation();
        split.swiped(40.0, 0.0, TouchPhase::Started, 400.0);
        split.swiped(40.0, 0.0, TouchPhase::Moved, 400.0);
        split.heading.set((40.0, Some(std::time::Instant::now() - FLICK_PAUSE * 2)));
        split.swiped(0.0, 0.0, TouchPhase::Ended, 400.0);
        assert!(split.showing_content(), "set down a fifth of the way: the conversation stays");
    }
}
