use super::constants::{CAMERA_BITRATE, MAX_FPS, SCREEN_BITRATE};

pub(super) fn camera_launch(source: &str) -> String {
    format!(
        "{source} ! videorate ! videoscale ! videoconvert \
         ! video/x-raw,width=640,height=480,framerate={MAX_FPS}/1 \
         ! vp8enc deadline=1 error-resilient=default keyframe-max-dist=60 \
           target-bitrate={CAMERA_BITRATE} \
         ! appsink name=out sync=false max-buffers=2 drop=true"
    )
}

pub(super) fn screen_launch(fd: std::os::fd::RawFd, node_id: u32) -> String {
    format!(
        "pipewiresrc fd={fd} path={node_id} do-timestamp=true \
         ! videorate ! videoconvert ! video/x-raw,framerate={MAX_FPS}/1 \
         ! vp8enc deadline=1 error-resilient=default keyframe-max-dist=60 \
           target-bitrate={SCREEN_BITRATE} \
         ! appsink name=out sync=false max-buffers=2 drop=true"
    )
}

pub(super) fn receive_launch(encoding: &str, decode: &str, payload_type: u8) -> String {
    format!(
        "appsrc name=in is-live=true format=time do-timestamp=true \
           caps=application/x-rtp,media=video,clock-rate=90000,\
encoding-name={encoding},payload={payload_type} \
         ! rtpjitterbuffer latency=150 ! {decode} ! videoconvert \
         ! appsink name=out caps=video/x-raw,format=BGRA \
           sync=false max-buffers=2 drop=true"
    )
}

#[cfg(test)]
mod tests {
    use gstreamer as gst;

    use super::super::init::init;
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
