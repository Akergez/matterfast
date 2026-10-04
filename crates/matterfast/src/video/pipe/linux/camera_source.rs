use gstreamer as gst;
use gstreamer::prelude::*;

use super::init::init;

/// The first camera GStreamer can really open.
///
/// `v4l2src` on its own takes `/dev/video0`, which on a phone is as likely
/// to be an ISP node as a camera — the PinePhone Pro answers there with
/// "not a capture device". The device monitor asks the v4l2, libcamera and
/// PipeWire providers what is actually attached instead of guessing.
pub(super) fn camera_source() -> Result<String, String> {
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
