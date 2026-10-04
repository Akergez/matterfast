//! Channels, posts and users on disk, so the window draws something real
//! before the first HTTP response lands.
//!
//! This is the upgrade [`crate::cache`] names in its own doc comment. The
//! snapshot there is one JSON file rewritten whole, which is why it could only
//! afford to keep the eight most recently active feeds (`cache::trim_feeds`):
//! the cost of a write scaled with everything you had ever read, not with what
//! had changed. Here a post is a row, so every channel keeps its history and a
//! write touches only the rows it names.
//!
//! Still a cache, not a source of truth: the network replaces all of it the
//! moment it answers, and any doubt about the file's contents is settled by
//! deleting it (see `ensure_schema`).

mod constants;
mod ensure_schema;
mod error;
mod handle;
mod host_of;
mod load;
mod open;
mod paths;
mod prune;
mod read_posts;
mod save;
#[cfg(test)]
mod test_support;

pub use constants::DEFAULT_KEEP_PER_CHANNEL;
pub use handle::Store;
