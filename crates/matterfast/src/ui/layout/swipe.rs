/// What a pan across the window is to the channel list.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Swipe {
    /// Somebody else's: a list being scrolled.
    Idle,
    /// The finger is carrying the channel list.
    Dragging,
    /// The finger has let go and the gesture's momentum is still arriving;
    /// the list is already on its way and the momentum is nobody's.
    Coasting,
}
