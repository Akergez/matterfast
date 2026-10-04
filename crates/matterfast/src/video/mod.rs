//! Video for a call: what we send (screen, camera) and what we show.
//!
//! GStreamer does the codec work at both ends, which is why the pipelines are
//! written as launch strings rather than assembled element by element:
//!
//! * **out** — `pipewiresrc`/`v4l2src` → VP8 → `appsink` → the call's track;
//! * **in** — the call's RTP → `appsrc` → VP8 → `appsink` → a frame to draw.
//!
//! Feeding raw RTP straight into `rtpvp8depay` is why there is no hand-written
//! depacketiser here: reassembly, reordering and frame boundaries are all
//! things `rtpjitterbuffer` already does correctly.
//!
//! That is Linux, the only place this is built with GStreamer so far.
//! Elsewhere [`pipe`] is its `stub` module, which answers every request with
//! an error: a call still works, with sound and without pictures.
//!
//! Nothing GStreamer-shaped leaves the [`pipe`] module: it owns every
//! GStreamer type and hands out plain [`Frame`]s, and only the rest of this
//! module knows there is a window to draw them in.

mod constants;
mod frame;
mod frame_image;
mod init_gstreamer;
mod pipe;
mod remote_view;
mod show_remote;

pub use frame::Frame;
pub use frame_image::frame_image;
#[cfg(target_os = "linux")]
pub use init_gstreamer::init_gstreamer;
pub use pipe::{pick_screen, VideoSender};
pub use remote_view::RemoteView;
pub use show_remote::show_remote;
