use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::RenderImage;

use super::constants::{BIG_HEIGHT, BIG_WIDTH, PIP_HEIGHT, PIP_WIDTH};
use super::pipe;

/// A remote video living in the corner of the window.
///
/// Dropping it stops the decoder, which is what should happen when the sharer
/// stops or the call ends; the picture goes with it, because the view draws
/// whatever views are still held.
pub struct RemoteView {
    pub title: String,
    /// Picture-in-picture until clicked, then large.
    pub expanded: Cell<bool>,
    pub(super) latest: Rc<RefCell<Option<Arc<RenderImage>>>>,
    pub(super) pipeline: pipe::Receiver,
}

impl RemoteView {
    /// The newest decoded frame, or nothing before the first one lands.
    pub fn image(&self) -> Option<Arc<RenderImage>> {
        self.latest.borrow().clone()
    }

    /// How large to draw it right now.
    pub fn size(&self) -> (f32, f32) {
        if self.expanded.get() {
            (BIG_WIDTH, BIG_HEIGHT)
        } else {
            (PIP_WIDTH, PIP_HEIGHT)
        }
    }
}

impl Drop for RemoteView {
    fn drop(&mut self) {
        self.pipeline.stop();
    }
}
