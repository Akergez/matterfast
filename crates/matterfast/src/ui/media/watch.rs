use std::rc::Rc;
use std::sync::Arc;

use super::constants::PROGRESS_TICK;
use super::stage::Stage;
use crate::playback;
use crate::runtime;
use crate::ui::{Part, Ui};

/// Where a player can be: under a message in the conversation or in the
/// panel beside it, or filling the window. Never in the channel list, which
/// is spared a redraw for every picture of a film.
const SHOWN_IN: &[Part] = &[Part::Chat, Part::Right, Part::Frame];

impl Ui {
    /// Follows a playback: its pictures, its end, its failure.
    pub(super) fn watch_media(
        self: &Rc<Self>,
        file_id: String,
        events: async_channel::Receiver<playback::Event>,
    ) {
        let weak = Rc::downgrade(self);
        runtime::receive(events, move |event, cx| {
            let Some(ui) = weak.upgrade() else { return false };
            let player = ui.media_player(&file_id);
            match event {
                playback::Event::Frame(frame) => {
                    if let Some(image) = crate::video::frame_image(frame) {
                        if let Some(old) = player.frame.replace(Some(Arc::new(image))) {
                            cx.drop_image(old, None);
                        }
                    }
                }
                playback::Event::Ended => player.stage.set(Stage::Ended),
                playback::Event::Failed(error) => player.failed(&error),
            }
            crate::ui::redraw(SHOWN_IN, cx);
            player.playback.borrow().is_some()
        });
    }

    /// Redraws now and then while something plays, so its clock moves. Audio
    /// sends no pictures to do it for us.
    pub(super) fn tick_media(self: &Rc<Self>, file_id: String) {
        let weak = Rc::downgrade(self);
        runtime::after(PROGRESS_TICK, move |cx| {
            let Some(ui) = weak.upgrade() else { return };
            if ui.media_player(&file_id).stage.get() == Stage::Playing {
                crate::ui::redraw(SHOWN_IN, cx);
                ui.tick_media(file_id);
            }
        });
    }
}
