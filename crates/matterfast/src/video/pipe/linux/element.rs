use gstreamer as gst;
use gstreamer::prelude::*;

pub(super) fn element<T: IsA<gst::Element>>(
    pipeline: &gst::Pipeline,
    name: &str,
) -> Result<T, String> {
    pipeline
        .by_name(name)
        .ok_or_else(|| format!("no element named {name}"))?
        .downcast::<T>()
        .map_err(|_| format!("{name} is not the expected element type"))
}
