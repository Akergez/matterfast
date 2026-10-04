use std::sync::Arc;

use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use mattermost_calls::{Marshal, TrackRemote};

use super::build::build;
use super::element::element;
use super::launch::receive_launch;
use super::play::play;
use super::receiver::Receiver;
use crate::runtime;
use crate::video::Frame;

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
