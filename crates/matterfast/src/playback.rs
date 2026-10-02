//! Playing an attached video or audio file, and taking a still from one.
//!
//! GStreamer does all of it. The picture comes out the same way a call's
//! remote video does — plain [`Frame`]s, blue-first, handed over a channel —
//! so the window draws both with the same few lines and nothing
//! GStreamer-shaped leaves this module.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use crate::video::Frame;

/// What a running playback reports.
pub enum Event {
    Frame(Frame),
    /// It played to the end.
    Ended,
    Failed(String),
}

/// One file being played. Dropping it stops it and removes the temporary file
/// it was playing from.
pub struct Playback {
    pipeline: gst::Element,
    path: PathBuf,
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The sink a video's pictures are pulled from: converted to the one pixel
/// format the window draws, and never more than a couple queued — a stale
/// frame is worth less than the latency of catching up on it.
const VIDEO_SINK: &str = "videoconvert ! appsink name=out caps=video/x-raw,format=BGRA \
                          max-buffers=2 drop=true";

fn frame_from(sample: &gst::Sample) -> Option<Frame> {
    let buffer = sample.buffer()?;
    let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
    let map = buffer.map_readable().ok()?;
    Some(Frame {
        pixels: map.to_vec(),
        width: info.width(),
        height: info.height(),
        stride: info.stride()[0] as usize,
    })
}

impl Playback {
    /// Starts playing the file at `path`, which this takes ownership of.
    pub fn start(path: PathBuf) -> Result<(Playback, async_channel::Receiver<Event>), String> {
        crate::video::init_gstreamer()?;
        let uri = gst::glib::filename_to_uri(&path, None).map_err(|e| e.to_string())?;
        let pipeline = gst::ElementFactory::make("playbin")
            .property("uri", uri.as_str())
            .build()
            .map_err(|e| e.to_string())?;

        let (tx, rx) = async_channel::bounded::<Event>(4);

        let sink = gst::parse::bin_from_description(VIDEO_SINK, true).map_err(|e| e.to_string())?;
        let appsink = sink
            .by_name("out")
            .and_then(|element| element.downcast::<gst_app::AppSink>().ok())
            .ok_or("the video sink has no appsink")?;
        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample({
                    let tx = tx.clone();
                    move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        // Dropped rather than queued when the window is
                        // behind; the next one is along in a few milliseconds.
                        if !tx.is_full() {
                            if let Some(frame) = frame_from(&sample) {
                                let _ = tx.try_send(Event::Frame(frame));
                            }
                        }
                        Ok(gst::FlowSuccess::Ok)
                    }
                })
                .build(),
        );
        pipeline.set_property("video-sink", &sink);

        // A pipeline that gives up reports it on its bus and nowhere else.
        if let Some(bus) = pipeline.bus() {
            bus.set_sync_handler(move |_, message| {
                match message.view() {
                    gst::MessageView::Eos(_) => {
                        let _ = tx.send_blocking(Event::Ended);
                    }
                    gst::MessageView::Error(error) => {
                        let _ = tx.send_blocking(Event::Failed(error.error().to_string()));
                    }
                    _ => {}
                }
                gst::BusSyncReply::Drop
            });
        }

        let playback = Playback { pipeline, path };
        playback
            .pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| e.to_string())?;
        Ok((playback, rx))
    }

    pub fn set_paused(&self, paused: bool) {
        let _ = self.pipeline.set_state(if paused {
            gst::State::Paused
        } else {
            gst::State::Playing
        });
    }

    /// Where it is and how long it is, in seconds. Nothing until the
    /// pipeline knows.
    pub fn progress(&self) -> Option<(f64, f64)> {
        let position = self.pipeline.query_position::<gst::ClockTime>()?;
        let duration = self.pipeline.query_duration::<gst::ClockTime>()?;
        Some((
            position.mseconds() as f64 / 1000.0,
            duration.mseconds() as f64 / 1000.0,
        ))
    }


    /// Back to the beginning, playing.
    pub fn restart(&self) {
        let _ = self.pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::ZERO,
        );
        self.set_paused(false);
    }
}

/// How long the decoder is given to produce the first frame. Generous: getting
/// nothing means the row keeps its play button, which is where it started.
const STILL_WAIT: Duration = Duration::from_millis(1500);

/// A still frame for a video, decoded from just the head of the file. `None`
/// when no picture could be had from it — a codec GStreamer does not have,
/// say — which is the "quietly do nothing" case: the play button still works,
/// because it downloads and plays the whole file rather than this stub.
///
/// Blocks for as long as the decoder takes; call it off the main thread.
pub fn still(head: &[u8], name: &Path) -> Option<Frame> {
    if std::env::var("MATTERFAST_NO_STILL").is_ok() {
        return None;
    }
    crate::video::init_gstreamer().ok()?;
    std::fs::write(name, head).ok()?;
    // The file is removed on every way out, including the early ones.
    struct Cleanup<'a>(&'a Path, Option<gst::Element>);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            if let Some(pipeline) = &self.1 {
                let _ = pipeline.set_state(gst::State::Null);
            }
            let _ = std::fs::remove_file(self.0);
        }
    }
    let mut cleanup = Cleanup(name, None);

    let pipeline = gst::parse::launch(&format!(
        "filesrc location=\"{}\" ! decodebin ! videoconvert \
         ! appsink name=out caps=video/x-raw,format=BGRA max-buffers=1",
        name.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"")
    ))
    .ok()?;
    cleanup.1 = Some(pipeline.clone());
    let sink = pipeline
        .downcast_ref::<gst::Bin>()?
        .by_name("out")?
        .downcast::<gst_app::AppSink>()
        .ok()?;
    // Paused is enough: the first frame is what a sink prerolls on.
    pipeline.set_state(gst::State::Paused).ok()?;
    let sample = sink.try_pull_preroll(gst::ClockTime::from_mseconds(
        STILL_WAIT.as_millis() as u64
    ))?;
    frame_from(&sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sink description is a string, so a typo in it is only caught when
    /// someone presses play. Parsing it is that check, minus a file.
    #[test]
    fn the_video_sink_parses() {
        crate::video::init_gstreamer().expect("gstreamer failed to initialise");
        let sink = gst::parse::bin_from_description(VIDEO_SINK, true)
            .unwrap_or_else(|e| panic!("{VIDEO_SINK}\n\nfailed to parse: {e}"));
        assert!(sink.by_name("out").is_some());
    }

    #[test]
    fn a_head_that_is_not_a_video_gives_no_still() {
        let path = std::env::temp_dir().join(format!("matterfast-still-test-{}", std::process::id()));
        assert!(still(b"this is not a video at all", &path).is_none());
        // And leaves nothing behind.
        assert!(!path.exists());
    }
}
