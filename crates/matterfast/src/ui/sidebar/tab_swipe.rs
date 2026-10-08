/// What a pan across the list of conversations is to its tabs.
///
/// On a phone the list is the main screen and a swipe across it goes to the
/// tab beside the one in front, as in Telegram. A swipe turns one tab however
/// far it goes: the tabs are not carried by the finger, so there is nothing
/// to show for a second one, and a flick would otherwise run through them all.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum TabSwipe {
    /// Somebody else's: the list being scrolled.
    #[default]
    Idle,
    /// Sideways, and this far so far.
    Tracking(f32),
    /// It has turned its tab, or was let go short of one; what is left of it
    /// — the momentum — is nobody's.
    Done,
}

impl TabSwipe {
    /// One step of a pan: where that leaves the swipe, and the tab to go to,
    /// as so many places along, if this step was the one that got there.
    /// `reach` is how far a swipe has to go to count.
    pub(super) fn pan(self, dx: f32, dy: f32, started: bool, reach: f32) -> (TabSwipe, Option<i32>) {
        // A pan is locked to one axis from its first step.
        let swipe = match started {
            true if dy == 0.0 && dx != 0.0 => TabSwipe::Tracking(0.0),
            true => TabSwipe::Idle,
            false => self,
        };
        let TabSwipe::Tracking(so_far) = swipe else {
            return (swipe, None);
        };
        let so_far = so_far + dx;
        if so_far.abs() < reach {
            return (TabSwipe::Tracking(so_far), None);
        }
        // The finger pulls the next tab in from the side it is going away
        // from: to the left is on to the next one.
        (TabSwipe::Done, Some(if so_far < 0.0 { 1 } else { -1 }))
    }

    /// Whether the pan is the tabs' and not a scroll for what is underneath.
    pub(super) fn taken(self) -> bool {
        self != TabSwipe::Idle
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
    fn a_swipe_to_the_left_goes_on_one_tab_once_it_has_gone_far_enough() {
        let (swipe, turn) = TabSwipe::Idle.pan(-20.0, 0.0, true, 50.0);
        assert_eq!((swipe, turn), (TabSwipe::Tracking(-20.0), None));
        let (swipe, turn) = swipe.pan(-40.0, 0.0, false, 50.0);
        assert_eq!((swipe, turn), (TabSwipe::Done, Some(1)));
    }

    #[test]
    fn a_swipe_to_the_right_goes_back_one() {
        let (swipe, _) = TabSwipe::Idle.pan(30.0, 0.0, true, 50.0);
        assert_eq!(swipe.pan(30.0, 0.0, false, 50.0), (TabSwipe::Done, Some(-1)));
    }

    #[test]
    fn a_swipe_turns_one_tab_however_far_it_goes() {
        let (swipe, _) = TabSwipe::Idle.pan(-80.0, 0.0, true, 50.0);
        assert_eq!(swipe, TabSwipe::Done);
        // The rest of it, and its momentum, is taken and does nothing.
        let (swipe, turn) = swipe.pan(-300.0, 0.0, false, 50.0);
        assert_eq!((swipe, turn), (TabSwipe::Done, None));
        assert!(swipe.taken());
    }

    #[test]
    fn a_scroll_of_the_list_is_not_a_swipe() {
        let (swipe, turn) = TabSwipe::Done.pan(0.0, -12.0, true, 50.0);
        assert_eq!((swipe, turn), (TabSwipe::Idle, None));
        assert!(!swipe.taken());
        // Drifting sideways later in the same pan does not make it one.
        assert_eq!(swipe.pan(-90.0, 0.0, false, 50.0), (TabSwipe::Idle, None));
    }

    #[test]
    fn the_first_and_the_last_tab_are_ends() {
        assert_eq!(turned(0, -1, 4), 0);
        assert_eq!(turned(3, 1, 4), 3);
        assert_eq!(turned(1, 1, 4), 2);
        assert_eq!(turned(0, 1, 0), 0);
    }
}
