/// What a player is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    /// A poster and a play button.
    Idle,
    /// The file is on its way.
    Loading,
    Playing,
    Paused,
    /// It reached the end; play starts it over.
    Ended,
}
