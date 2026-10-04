use std::cell::{Cell, RefCell};
use std::sync::Arc;

use gpui_kit::RenderImage;

use super::stage::Stage;
use crate::playback::Playback;

/// A player for one attached video or audio file. It belongs to the session
/// rather than to the row that draws it, so scrolling the message away does
/// not stop the music.
pub struct Player {
    pub(super) stage: Cell<Stage>,
    /// The newest decoded picture of a playing video.
    pub(super) frame: RefCell<Option<Arc<RenderImage>>>,
    pub(super) playback: RefCell<Option<Playback>>,
    /// What went wrong, in the words GStreamer or the network used.
    pub(super) error: RefCell<Option<String>>,
    /// Drawn over the whole window rather than in the row.
    pub(super) expanded: Cell<bool>,
}

impl Player {
    pub(super) fn new() -> Self {
        Player {
            stage: Cell::new(Stage::Idle),
            frame: RefCell::new(None),
            playback: RefCell::new(None),
            error: RefCell::new(None),
            expanded: Cell::new(false),
        }
    }

    pub(super) fn failed(&self, message: &str) {
        tracing::warn!(message, "could not play the attachment");
        self.playback.borrow_mut().take();
        self.stage.set(Stage::Idle);
        *self.error.borrow_mut() = Some(message.to_string());
    }

    /// The picture to draw while this is the expanded one, if it has one.
    pub fn expanded_frame(&self) -> Option<Arc<RenderImage>> {
        self.expanded.get().then(|| self.frame.borrow().clone()).flatten()
    }

    pub fn collapse(&self) {
        self.expanded.set(false);
    }
}
