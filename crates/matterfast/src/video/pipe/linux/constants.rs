/// Video we send is capped here. A screen share at native resolution and
/// 60 fps is mostly wasted on a viewer's window, and the encoder is the one
/// thing in this pipeline that can saturate a core.
pub(super) const MAX_FPS: u32 = 30;
pub(super) const SCREEN_BITRATE: u32 = 1_500_000;
pub(super) const CAMERA_BITRATE: u32 = 600_000;
