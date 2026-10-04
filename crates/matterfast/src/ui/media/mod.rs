//! Playing an attached video or audio file where it was posted.
//!
//! A [`Player`] starts as a poster — a still, or failing that just the name,
//! the size and a play button — and only downloads the *whole* file when
//! someone presses play, because a channel scrolled past ten videos would
//! otherwise fetch ten videos. The still itself costs far less: the server
//! makes no thumbnail for video (it 400s `no_thumbnail`, confirmed against a
//! live server), so one is built from just the head of the file — see
//! [`head_playable`] for how much that is and which containers it works for.
//!
//! The file itself is behind the session token, so a URL handed to GStreamer
//! would come back 401; the bytes arrive the same way an image's do, through
//! the client, and are written to a temporary file to play from.

mod clock;
mod constants;
mod draw;
mod formats;
mod head_playable;
mod play;
mod player;
mod poster;
mod registry;
mod sanitised;
mod stage;
mod watch;

pub use draw::player;
pub use formats::is_playable;
pub use player::Player;
