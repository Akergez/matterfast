//! The Zed extension registry's side of the fake: one theme, listed and
//! downloadable.

mod download;
mod extensions;
mod theme;

pub(crate) use download::zed_extension_download;
pub(crate) use extensions::zed_extensions;
