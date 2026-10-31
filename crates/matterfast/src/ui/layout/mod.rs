//! Where the panes sit in the window, and the window itself.

mod divider;
mod pages;
mod widths;
mod window_slot;

pub use divider::Divider;
pub use pages::{Pages, Place};
pub use widths::Widths;
pub use window_slot::WindowSlot;
