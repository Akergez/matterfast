//! Profile picture cache.
//!
//! Neither the download nor the decode may block the UI, so both happen on
//! Tokio: the bytes are fetched, decoded and scaled there, and what comes back
//! to the main thread is a finished [`RenderImage`](gpui_kit::RenderImage),
//! kept in a plain `RefCell` map. Everything that touches that map therefore
//! runs on the main thread and needs no locking beyond that.
//!
//! The server always returns *an* image — it renders default initials for users
//! who never uploaded one — so there is no "user has no avatar" case to handle.
//! There is only "not fetched yet", which shows initials until the picture
//! lands.

mod constants;
mod decode;
mod decode_animation;
mod fetched;
mod forget;
mod inner;
mod loaded_callback;
mod lookup;
mod new;
mod release;
mod render_image;
mod request;
mod service;
mod texture_bytes;

pub use render_image::render_image;
pub use service::Avatars;
