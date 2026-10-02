//! Video for a call: what we send (screen, camera) and what we show.
//!
//! GStreamer does the codec work at both ends, which is why the pipelines are
//! written as launch strings rather than assembled element by element:
//!
//! * **out** — `pipewiresrc`/`v4l2src` → VP8 → `appsink` → the call's track;
//! * **in** — the call's RTP → `appsrc` → VP8 → `appsink` → a frame to draw.
//!
//! Feeding raw RTP straight into `rtpvp8depay` is why there is no hand-written
//! depacketiser here: reassembly, reordering and frame boundaries are all
//! things `rtpjitterbuffer` already does correctly.
//!
//! Nothing GStreamer-shaped leaves the [`pipe`] module: it owns every
//! GStreamer type and hands out plain [`Frame`]s, and only this outer module
//! knows there is a window to draw them in.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{App, RenderImage};
use mattermost_calls::TrackRemote;

/// One decoded picture, in the only form both halves agree on: BGRA, which is
/// what the renderer uploads without another pass over the pixels.
pub struct Frame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
}

pub use pipe::{pick_screen, VideoSender};

/// A remote video living in the corner of the window.
///
/// Dropping it stops the decoder, which is what should happen when the sharer
/// stops or the call ends; the picture goes with it, because the view draws
/// whatever views are still held.
pub struct RemoteView {
    pub title: String,
    /// Picture-in-picture until clicked, then large.
    pub expanded: Cell<bool>,
    latest: Rc<RefCell<Option<Arc<RenderImage>>>>,
    pipeline: pipe::Receiver,
}

impl RemoteView {
    /// The newest decoded frame, or nothing before the first one lands.
    pub fn image(&self) -> Option<Arc<RenderImage>> {
        self.latest.borrow().clone()
    }

    /// How large to draw it right now.
    pub fn size(&self) -> (f32, f32) {
        if self.expanded.get() {
            (BIG_WIDTH, BIG_HEIGHT)
        } else {
            (PIP_WIDTH, PIP_HEIGHT)
        }
    }
}

impl Drop for RemoteView {
    fn drop(&mut self) {
        self.pipeline.stop();
    }
}

/// Picture-in-picture size. Expanded is a fixed size rather than a fill so
/// that several shares can be open at once without fighting over the overlay.
const PIP_WIDTH: f32 = 260.0;
const PIP_HEIGHT: f32 = 146.0;
const BIG_WIDTH: f32 = 960.0;
const BIG_HEIGHT: f32 = 540.0;

/// Turns a decoded frame into something the renderer can draw. `None` for a
/// frame whose buffer does not hold what its own header claims.
fn render_image(frame: Frame) -> Option<RenderImage> {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let row = width * 4;
    let pixels = if frame.stride == row {
        let mut pixels = frame.pixels;
        pixels.truncate(row * height);
        pixels
    } else {
        // Padded rows: the renderer wants them packed.
        let mut packed = Vec::with_capacity(row * height);
        for line in frame.pixels.chunks(frame.stride).take(height) {
            packed.extend_from_slice(line.get(..row)?);
        }
        packed
    };
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, pixels)?;
    Some(RenderImage::new(vec![image::Frame::new(buffer)]))
}

/// Starts decoding a remote track. `on_frame` runs on the main thread each
/// time there is a new picture to draw.
pub fn show_remote(
    title: &str,
    track: Arc<TrackRemote>,
    on_frame: impl Fn(&mut App) + 'static,
) -> Result<RemoteView, String> {
    let (pipeline, frames) = pipe::receive(track)?;
    let latest: Rc<RefCell<Option<Arc<RenderImage>>>> = Rc::new(RefCell::new(None));

    let slot = Rc::downgrade(&latest);
    let mut first = true;
    crate::runtime::receive(frames, move |frame, cx| {
        // The view went away; stop pulling frames for nobody.
        let Some(slot) = slot.upgrade() else {
            return false;
        };
        if std::mem::take(&mut first) {
            tracing::info!(
                width = frame.width,
                height = frame.height,
                "the first frame of a remote video decoded"
            );
        }
        let Some(image) = render_image(frame) else {
            return true;
        };
        // The previous frame's texture is given back explicitly: the renderer
        // keeps what it has uploaded until told otherwise, and thirty of
        // these a second is not something to leave lying around.
        if let Some(old) = slot.replace(Some(Arc::new(image))) {
            cx.drop_image(old, None);
        }
        on_frame(cx);
        true
    });

    Ok(RemoteView {
        title: title.to_string(),
        expanded: Cell::new(false),
        latest,
        pipeline,
    })
}

/// Starts GStreamer, once. Everything that builds a pipeline asks first.
pub fn init_gstreamer() -> Result<(), String> {
    pipe::init()
}

/// Turns a decoded frame into something the renderer can draw; see
/// [`render_image`].
pub fn frame_image(frame: Frame) -> Option<RenderImage> {
    render_image(frame)
}

/// Everything that speaks GStreamer.
mod pipe {
    use std::sync::Arc;
    use std::time::Duration;

    use gstreamer as gst;
    use gstreamer::prelude::*;
    use gstreamer_app as gst_app;
    use gstreamer_video as gst_video;
    use mattermost_calls::{Marshal, Sample, TrackLocalStaticSample, TrackRemote};

    use super::Frame;
    use crate::runtime;

    /// Video we send is capped here. A screen share at native resolution and
    /// 60 fps is mostly wasted on a viewer's window, and the encoder is the one
    /// thing in this pipeline that can saturate a core.
    const MAX_FPS: u32 = 30;
    const SCREEN_BITRATE: u32 = 1_500_000;
    const CAMERA_BITRATE: u32 = 600_000;

    pub(super) fn init() -> Result<(), String> {
        static ONCE: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| gst::init().map_err(|e| e.to_string()))
            .clone()
    }

    /// A pipeline someone else's window is responsible for stopping.
    pub struct Receiver(gst::Pipeline);

    impl Receiver {
        pub fn stop(&self) {
            let _ = self.0.set_state(gst::State::Null);
        }
    }

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

    /// Decodes a remote track into a stream of frames.
    pub fn receive(
        track: Arc<TrackRemote>,
    ) -> Result<(Receiver, async_channel::Receiver<Frame>), String> {
        let codec = track.codec();
        let mime = codec.capability.mime_type.to_lowercase();
        // The SFU registers exactly these two video codecs.
        let (encoding, decode) = match mime.as_str() {
            "video/vp8" => ("VP8", "rtpvp8depay ! vp8dec"),
            "video/av1" => ("AV1", "rtpav1depay ! av1dec"),
            other => return Err(format!("no decoder for {other}")),
        };

        let pipeline = build(&receive_launch(encoding, decode, codec.payload_type))?;
        let src = element::<gst_app::AppSrc>(&pipeline, "in")?;
        let sink = element::<gst_app::AppSink>(&pipeline, "out")?;

        // Frames are dropped rather than queued when the UI falls behind: a
        // stale frame is worth less than the latency of catching up on it.
        let (tx, rx) = async_channel::bounded::<Frame>(1);
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if tx.is_full() {
                        return Ok(gst::FlowSuccess::Ok);
                    }
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;
                    let info =
                        gst_video::VideoInfo::from_caps(caps).map_err(|_| gst::FlowError::Error)?;
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let _ = tx.try_send(Frame {
                        pixels: map.to_vec(),
                        width: info.width(),
                        height: info.height(),
                        stride: info.stride()[0] as usize,
                    });
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        // The RTP the SFU sends us, byte for byte, is what the depayloader wants.
        runtime::runtime().spawn({
            let src = src.clone();
            async move {
                while let Ok((packet, _)) = track.read_rtp().await {
                    let Ok(raw) = packet.marshal() else { continue };
                    if src.push_buffer(gst::Buffer::from_slice(raw)).is_err() {
                        break;
                    }
                }
                let _ = src.end_of_stream();
            }
        });

        play(&pipeline)?;
        Ok((Receiver(pipeline), rx))
    }

    /// Asks the desktop portal for a screen or window to share.
    ///
    /// Wayland has no other way in: an app cannot read the screen itself, it
    /// can only be handed a PipeWire node by the compositor after the user
    /// picks one.
    pub async fn pick_screen() -> Result<(u32, std::os::fd::OwnedFd), String> {
        use ashpd::desktop::screencast::{
            CursorMode, Screencast, SelectSourcesOptions, SourceType,
        };

        let proxy = Screencast::new().await.map_err(|e| e.to_string())?;
        let session = proxy
            .create_session(Default::default())
            .await
            .map_err(|e| e.to_string())?;
        proxy
            .select_sources(
                &session,
                SelectSourcesOptions::default()
                    .set_cursor_mode(CursorMode::Embedded)
                    .set_sources(SourceType::Monitor | SourceType::Window)
                    .set_multiple(false),
            )
            .await
            .map_err(|e| e.to_string())?;
        let response = proxy
            .start(&session, None, Default::default())
            .await
            .map_err(|e| e.to_string())?
            .response()
            .map_err(|e| e.to_string())?;
        let stream = response
            .streams()
            .first()
            .ok_or("the portal returned no stream")?;
        let node_id = stream.pipe_wire_node_id();
        let fd = proxy
            .open_pipe_wire_remote(&session, Default::default())
            .await
            .map_err(|e| e.to_string())?;
        Ok((node_id, fd))
    }

    /// The first camera GStreamer can really open.
    ///
    /// `v4l2src` on its own takes `/dev/video0`, which on a phone is as likely
    /// to be an ISP node as a camera — the PinePhone Pro answers there with
    /// "not a capture device". The device monitor asks the v4l2, libcamera and
    /// PipeWire providers what is actually attached instead of guessing.
    fn camera_source() -> Result<String, String> {
        init()?;
        let monitor = gst::DeviceMonitor::new();
        monitor
            .add_filter(Some("Video/Source"), None)
            .ok_or("could not look for a camera")?;
        monitor.start().map_err(|e| e.to_string())?;
        let device = monitor.devices().into_iter().next();
        monitor.stop();
        let device = device.ok_or("no camera is available")?;
        let factory = device
            .create_element(None)
            .map_err(|e| e.to_string())?
            .factory()
            .ok_or("the camera would not open")?
            .name()
            .to_string();
        // Only v4l2 needs the node spelled out; libcamera and PipeWire sources
        // address the camera themselves.
        match device
            .properties()
            .and_then(|p| p.get::<String>("device.path").ok())
        {
            Some(path) => Ok(format!("{factory} device={path}")),
            None => Ok(factory),
        }
    }

    fn camera_launch(source: &str) -> String {
        format!(
            "{source} ! videorate ! videoscale ! videoconvert \
             ! video/x-raw,width=640,height=480,framerate={MAX_FPS}/1 \
             ! vp8enc deadline=1 error-resilient=default keyframe-max-dist=60 \
               target-bitrate={CAMERA_BITRATE} \
             ! appsink name=out sync=false max-buffers=2 drop=true"
        )
    }

    fn screen_launch(fd: std::os::fd::RawFd, node_id: u32) -> String {
        format!(
            "pipewiresrc fd={fd} path={node_id} do-timestamp=true \
             ! videorate ! videoconvert ! video/x-raw,framerate={MAX_FPS}/1 \
             ! vp8enc deadline=1 error-resilient=default keyframe-max-dist=60 \
               target-bitrate={SCREEN_BITRATE} \
             ! appsink name=out sync=false max-buffers=2 drop=true"
        )
    }

    fn receive_launch(encoding: &str, decode: &str, payload_type: u8) -> String {
        format!(
            "appsrc name=in is-live=true format=time do-timestamp=true \
               caps=application/x-rtp,media=video,clock-rate=90000,\
encoding-name={encoding},payload={payload_type} \
             ! rtpjitterbuffer latency=150 ! {decode} ! videoconvert \
             ! appsink name=out caps=video/x-raw,format=BGRA \
               sync=false max-buffers=2 drop=true"
        )
    }

    fn build(launch: &str) -> Result<gst::Pipeline, String> {
        init()?;
        let pipeline = gst::parse::launch(launch)
            .map_err(|e| e.to_string())?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "not a pipeline".to_string())?;
        // A pipeline that gives up reports it on its bus and nowhere else:
        // `set_state` returned Ok long before, so without this watch a decoder
        // that died is indistinguishable from a peer who sends nothing.
        if let Some(bus) = pipeline.bus() {
            // A sync handler rather than a watch: a watch is dispatched by a
            // GLib main loop, and nothing in this process runs one.
            bus.set_sync_handler(|_, msg| {
                if let gst::MessageView::Error(err) = msg.view() {
                    tracing::warn!(
                        source = %msg.src().map(|s| s.path_string()).unwrap_or_default(),
                        error = %err.error(),
                        debug = %err.debug().unwrap_or_default(),
                        "a video pipeline failed",
                    );
                }
                gst::BusSyncReply::Drop
            });
        }
        Ok(pipeline)
    }

    fn play(pipeline: &gst::Pipeline) -> Result<(), String> {
        pipeline
            .set_state(gst::State::Playing)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn element<T: IsA<gst::Element>>(pipeline: &gst::Pipeline, name: &str) -> Result<T, String> {
        pipeline
            .by_name(name)
            .ok_or_else(|| format!("no element named {name}"))?
            .downcast::<T>()
            .map_err(|_| format!("{name} is not the expected element type"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every pipeline here is a string, so a typo in an element name or a
        /// property value is only caught when someone starts a call. Parsing
        /// them is that check, minus the hardware.
        #[test]
        fn every_pipeline_parses() {
            init().expect("gstreamer failed to initialise");
            for launch in [
                camera_launch("v4l2src"),
                screen_launch(3, 42),
                receive_launch("VP8", "rtpvp8depay ! vp8dec", 96),
            ] {
                gst::parse::launch(&launch)
                    .unwrap_or_else(|e| panic!("{launch}\n\nfailed to parse: {e}"));
            }
        }
    }
}
