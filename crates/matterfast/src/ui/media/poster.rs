use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::RenderImage;
use mattermost_api::models::FileInfo;

use super::formats::extension;
use super::head_playable::head_playable;
use super::sanitised::sanitised;
use crate::playback;
use crate::runtime;
use crate::ui::Ui;

impl Ui {
    /// The still for a video, when there is one yet. Asking is what starts it
    /// being made: the head of the file is fetched, and a frame decoded from
    /// it off the main thread.
    pub(super) fn poster(self: &Rc<Self>, file: &FileInfo) -> Option<Arc<RenderImage>> {
        if let Some(poster) = self.avatars.poster(&file.id) {
            return Some(poster);
        }
        let head = self.avatars.video_head(&file.id)?;
        // A container whose index is not in the head cannot give a picture,
        // and fetching more of a multi-hundred-megabyte file for a thumbnail
        // is not worth it. Tried once either way.
        if !head_playable(&head) || !self.stills_tried.borrow_mut().insert(file.id.clone()) {
            return None;
        }
        let path = std::env::temp_dir().join(format!(
            "matterfast-poster-{}.{}",
            sanitised(&file.id),
            sanitised(&extension(file))
        ));
        let bytes = head.as_ref().clone();
        let ui = self.clone();
        let file_id = file.id.clone();
        runtime::spawn(
            async move {
                tokio::task::spawn_blocking(move || {
                    playback::still(&bytes, &path).and_then(crate::video::frame_image)
                })
                .await
                .ok()
                .flatten()
            },
            move |still, cx| {
                // Kept, so the next draw of that row shows a picture instead
                // of starting a decoder over again.
                if let Some(still) = still {
                    ui.avatars.remember_poster(&file_id, Arc::new(still), cx);
                    crate::ui::refresh(cx);
                }
            },
        );
        None
    }
}
