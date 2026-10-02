//! `video::pipe` where there is no GStreamer to speak to.
//!
//! The same names as the real module, so nothing above it has to know: every
//! attempt to send or show video fails with one sentence a person can read.

use std::sync::Arc;

use mattermost_calls::{TrackLocalStaticSample, TrackRemote};

use super::Frame;

const UNAVAILABLE: &str = "video is not available in this build";

pub(super) fn init() -> Result<(), String> {
    Err(UNAVAILABLE.into())
}

pub struct Receiver;

impl Receiver {
    pub fn stop(&self) {}
}

/// What `pick_screen` would hand to `VideoSender::screen`; never made.
pub struct ScreenSource;

pub struct VideoSender;

impl VideoSender {
    pub fn camera(_track: Arc<TrackLocalStaticSample>) -> Result<VideoSender, String> {
        Err(UNAVAILABLE.into())
    }

    pub fn screen(
        _track: Arc<TrackLocalStaticSample>,
        _node_id: u32,
        _source: ScreenSource,
    ) -> Result<VideoSender, String> {
        Err(UNAVAILABLE.into())
    }
}

pub fn receive(
    _track: Arc<TrackRemote>,
) -> Result<(Receiver, async_channel::Receiver<Frame>), String> {
    Err(UNAVAILABLE.into())
}

pub async fn pick_screen() -> Result<(u32, ScreenSource), String> {
    Err(UNAVAILABLE.into())
}
