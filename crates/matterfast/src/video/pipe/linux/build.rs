use gstreamer as gst;
use gstreamer::prelude::*;

use super::init::init;

pub(super) fn build(launch: &str) -> Result<gst::Pipeline, String> {
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
