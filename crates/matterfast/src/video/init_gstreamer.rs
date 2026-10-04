use super::pipe;

/// Starts GStreamer, once. Everything that builds a pipeline asks first.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn init_gstreamer() -> Result<(), String> {
    pipe::init()
}
