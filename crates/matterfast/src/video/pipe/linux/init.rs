use gstreamer as gst;

pub fn init() -> Result<(), String> {
    static ONCE: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| gst::init().map_err(|e| e.to_string()))
        .clone()
}
