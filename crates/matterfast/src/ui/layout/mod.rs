//! Where the panes sit in the window, and the window itself.

mod divider;
mod overlay;
mod split;
mod swipe;
mod widths;
mod window_slot;

pub use divider::Divider;
pub use overlay::Overlay;
pub use split::Split;
pub use widths::Widths;
pub use window_slot::WindowSlot;
