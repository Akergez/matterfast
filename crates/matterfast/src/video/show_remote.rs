use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{App, RenderImage};
use mattermost_calls::TrackRemote;

use super::frame_image::frame_image;
use super::pipe;
use super::remote_view::RemoteView;

/// Starts decoding a remote track. `on_frame` runs on the main thread each
/// time there is a new picture to draw.
pub fn show_remote(
    title: &str,
    track: Arc<TrackRemote>,
    on_frame: impl Fn(&mut App) + 'static,
) -> Result<RemoteView, String> {
    let (pipeline, frames) = pipe::receive(track)?;
    let latest: Rc<RefCell<Option<Arc<RenderImage>>>> = Rc::new(RefCell::new(None));

    let slot = Rc::downgrade(&latest);
    let mut first = true;
    crate::runtime::receive(frames, move |frame, cx| {
        // The view went away; stop pulling frames for nobody.
        let Some(slot) = slot.upgrade() else {
            return false;
        };
        if std::mem::take(&mut first) {
            tracing::info!(
                width = frame.width,
                height = frame.height,
                "the first frame of a remote video decoded"
            );
        }
        let Some(image) = frame_image(frame) else {
            return true;
        };
        // The previous frame's texture is given back explicitly: the renderer
        // keeps what it has uploaded until told otherwise, and thirty of
        // these a second is not something to leave lying around.
        if let Some(old) = slot.replace(Some(Arc::new(image))) {
            cx.drop_image(old, None);
        }
        on_frame(cx);
        true
    });

    Ok(RemoteView {
        title: title.to_string(),
        expanded: Cell::new(false),
        latest,
        pipeline,
    })
}
