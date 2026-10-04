use std::rc::Rc;

use gpui_kit::{App, ClipboardEntry};

use crate::ui::{Action, Ui};

/// Ctrl+V with an image on the clipboard attaches it. Pasting a screenshot is
/// how most images get into a chat, and the alternative is saving it to disk
/// first for no reason. Answers whether the paste was taken: when there is no
/// image, the text paste has to go through untouched.
pub(super) fn paste_image(ui: &Rc<Ui>, item: &gpui_kit::ClipboardItem, cx: &mut App) -> bool {
    let Some(image) = item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image.clone()),
        _ => None,
    }) else {
        return false;
    };
    let extension = match image.format {
        gpui_kit::ImageFormat::Png => "png",
        gpui_kit::ImageFormat::Jpeg => "jpg",
        gpui_kit::ImageFormat::Webp => "webp",
        gpui_kit::ImageFormat::Gif => "gif",
        gpui_kit::ImageFormat::Bmp => "bmp",
        _ => return false,
    };
    // The upload path takes paths, so the pasted image lands in a temp file
    // that the OS cleans up.
    let path = std::env::temp_dir().join(format!(
        "matterfast-paste-{}.{extension}",
        crate::timefmt::unique()
    ));
    if let Err(error) = std::fs::write(&path, &image.bytes) {
        tracing::warn!(%error, "could not save the pasted image");
        return false;
    }
    ui.dispatch(Action::AttachFiles(vec![path]), cx);
    true
}
