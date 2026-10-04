use gstreamer as gst;
use gstreamer::prelude::*;

pub(super) fn play(pipeline: &gst::Pipeline) -> Result<(), String> {
    pipeline
        .set_state(gst::State::Playing)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
