//! `playback` where there is no GStreamer to play with.
//!
//! The same names as the real module. Starting a playback fails with a
//! sentence the attachment row shows, and no still is ever produced, which
//! the row already treats as "leave the play button as it is".

use std::path::{Path, PathBuf};

use crate::video::Frame;

/// What a running playback reports.
#[allow(dead_code)]
pub enum Event {
    Frame(Frame),
    Ended,
    Failed(String),
}

pub struct Playback;

impl Playback {
    pub fn start(path: PathBuf) -> Result<(Playback, async_channel::Receiver<Event>), String> {
        // The real one owns the temporary file and removes it when dropped.
        let _ = std::fs::remove_file(path);
        Err("playing media is not available in this build".into())
    }

    pub fn set_paused(&self, _paused: bool) {}

    pub fn progress(&self) -> Option<(f64, f64)> {
        None
    }

    pub fn restart(&self) {}
}

pub fn still(_head: &[u8], _name: &Path) -> Option<Frame> {
    None
}
