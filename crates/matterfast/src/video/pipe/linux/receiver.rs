use gstreamer as gst;
use gstreamer::prelude::*;

/// A pipeline someone else's window is responsible for stopping.
pub struct Receiver(pub(super) gst::Pipeline);

impl Receiver {
    pub fn stop(&self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}
