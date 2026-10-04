use std::time::Duration;

/// The still shown before a video is played, and the picture while it is.
/// Matches the size an image attachment is drawn at, so a channel of clips
/// and photos reads evenly.
pub(super) const POSTER_WIDTH: f32 = 420.0;
pub(super) const POSTER_HEIGHT: f32 = 260.0;

/// How often a playing file's position is read. It is only a label.
pub(super) const PROGRESS_TICK: Duration = Duration::from_millis(500);
