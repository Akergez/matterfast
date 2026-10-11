/// A category gesture keeps its displacement until the finger is lifted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum TabSwipe {
    #[default]
    Idle,
    Tracking(f32),
    Done,
}

impl TabSwipe {
    pub(super) fn pan(self, dx: f32, dy: f32, started: bool, initial: f32) -> Self {
        let swipe = if started {
            if dx.abs() > dy.abs() {
                Self::Tracking(initial)
            } else {
                Self::Idle
            }
        } else {
            self
        };
        match swipe {
            Self::Tracking(offset) => Self::Tracking(offset + dx),
            other => other,
        }
    }

    pub(super) fn release(self, cancelled: bool) -> (Self, Option<i32>) {
        match self {
            Self::Tracking(offset) => {
                let turn = (!cancelled && offset.abs() >= 0.3).then_some(if offset < 0.0 {
                    1
                } else {
                    -1
                });
                (Self::Done, turn)
            }
            other => (other, None),
        }
    }

    pub(super) fn taken(self) -> bool {
        self != Self::Idle
    }
}

/// The tab `step` places along from `now`, of `count`: the ends are ends, and
/// a swipe past one stays on it.
pub(super) fn turned(now: usize, step: i32, count: usize) -> usize {
    (now as i64 + i64::from(step)).clamp(0, count.saturating_sub(1) as i64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displacement_follows_the_finger_and_can_reverse_before_release() {
        let swipe = TabSwipe::Idle.pan(-0.4, 0.0, true, 0.0);
        assert_eq!(swipe, TabSwipe::Tracking(-0.4));
        let swipe = swipe.pan(0.3, 0.0, false, 0.0);
        assert!(matches!(swipe, TabSwipe::Tracking(offset) if (offset + 0.1).abs() < 0.001));
        assert_eq!(swipe.release(false), (TabSwipe::Done, None));
    }

    #[test]
    fn turns_only_on_release() {
        assert_eq!(
            TabSwipe::Tracking(-0.4).release(false),
            (TabSwipe::Done, Some(1))
        );
        assert_eq!(
            TabSwipe::Tracking(0.4).release(false),
            (TabSwipe::Done, Some(-1))
        );
    }

    #[test]
    fn cancellation_returns_to_the_original_category() {
        assert_eq!(
            TabSwipe::Tracking(-0.8).release(true),
            (TabSwipe::Done, None)
        );
    }

    #[test]
    fn vertical_scroll_stays_vertical_and_momentum_cannot_turn_a_tab() {
        let swipe = TabSwipe::Done.pan(0.01, -0.1, true, 0.0);
        assert_eq!(swipe.pan(-0.9, 0.0, false, 0.0), TabSwipe::Idle);
        assert!(!swipe.taken());
        assert_eq!(TabSwipe::Done.pan(-0.9, 0.0, false, 0.0), TabSwipe::Done);
    }

    #[test]
    fn an_interrupted_settle_continues_at_its_current_position() {
        assert_eq!(
            TabSwipe::Done.pan(-0.1, 0.0, true, 0.5),
            TabSwipe::Tracking(0.4)
        );
    }

    #[test]
    fn the_first_and_the_last_tab_are_ends() {
        assert_eq!(turned(0, -1, 4), 0);
        assert_eq!(turned(3, 1, 4), 3);
        assert_eq!(turned(1, 1, 4), 2);
        assert_eq!(turned(0, 1, 0), 0);
    }
}
