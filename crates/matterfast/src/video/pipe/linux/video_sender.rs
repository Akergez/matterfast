use std::sync::Arc;
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use mattermost_calls::{Sample, TrackLocalStaticSample};

use super::build::build;
use super::camera_source::camera_source;
use super::constants::MAX_FPS;
use super::element::element;
use super::launch::{camera_launch, screen_launch};
use super::play::play;
use crate::runtime;

/// A running outbound pipeline. Drop it to stop sending.
pub struct VideoSender {
    pipeline: gst::Pipeline,
    /// The portal's PipeWire connection, which `pipewiresrc` reads through.
    /// Closing it would kill the capture, so it lives as long as the sender.
    _portal_fd: Option<std::os::fd::OwnedFd>,
}

impl Drop for VideoSender {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl VideoSender {
    /// Encodes the camera into `track`.
    pub fn camera(track: Arc<TrackLocalStaticSample>) -> Result<VideoSender, String> {
        Self::spawn(&camera_launch(&camera_source()?), track, None)
    }

    /// Encodes a PipeWire node — the screen or window the portal handed us.
    pub fn screen(
        track: Arc<TrackLocalStaticSample>,
        node_id: u32,
        fd: std::os::fd::OwnedFd,
    ) -> Result<VideoSender, String> {
        use std::os::fd::AsRawFd;
        let launch = screen_launch(fd.as_raw_fd(), node_id);
        Self::spawn(&launch, track, Some(fd))
    }

    fn spawn(
        launch: &str,
        track: Arc<TrackLocalStaticSample>,
        portal_fd: Option<std::os::fd::OwnedFd>,
    ) -> Result<VideoSender, String> {
        let pipeline = build(launch)?;
        let sink = element::<gst_app::AppSink>(&pipeline, "out")?;

        // The encoder runs on a GStreamer thread and the track is written
        // from Tokio, so the handover is a channel rather than a blocking
        // call.
        // Two frames are enough to overlap encoding and WebRTC. An
        // unbounded queue turns temporary network backpressure into both
        // growing memory and seconds of stale video.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(Vec<u8>, Duration)>(2);
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if tx.capacity() == 0 {
                        return Ok(gst::FlowSuccess::Ok);
                    }
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let duration = buffer
                        .duration()
                        .map(|d| Duration::from_nanos(d.nseconds()))
                        .unwrap_or(Duration::from_millis(1000 / MAX_FPS as u64));
                    let _ = tx.try_send((map.to_vec(), duration));
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        runtime::runtime().spawn(async move {
            let mut frames = 0u64;
            while let Some((data, duration)) = rx.recv().await {
                frames += 1;
                if frames % 30 == 0 {
                    tracing::debug!(frames, bytes = data.len(), "sending video");
                }
                let sample = Sample {
                    data: data.into(),
                    duration,
                    ..Default::default()
                };
                if let Err(e) = track.write_sample(&sample).await {
                    tracing::warn!(error = %e, "could not send video");
                    break;
                }
            }
        });

        play(&pipeline)?;
        Ok(VideoSender {
            pipeline,
            _portal_fd: portal_fd,
        })
    }
}
