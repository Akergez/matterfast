use std::sync::Arc;

use gpui_kit::{App, RenderImage};

/// Lets the renderer forget pictures the cache has let go of.
pub(super) fn release(dropped: Vec<Arc<RenderImage>>, cx: &mut App) {
    for image in dropped {
        cx.drop_image(image, None);
    }
}
