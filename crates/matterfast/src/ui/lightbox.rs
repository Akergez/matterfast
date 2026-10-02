//! The full-size view of an attached image, drawn over the window instead of
//! in a window of its own.
//!
//! A second window opens wherever the compositor feels like putting it, needs
//! a title bar to be closable at all, and reads as a different app. A lightbox
//! is what people expect from a picture on the web — it covers what you were
//! looking at, and a click next to it puts you back.
//!
//! The same surface shows a playing video when it is asked to fill the window.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{
    div, img, AnyElement, App, ClipboardItem, Image, ImageFormat, ObjectFit, RenderImage,
};
use mattermost_api::models::FileInfo;

use super::kit::{self, Lucide};
use super::Ui;
use crate::runtime;

/// One image being looked at.
pub struct Open {
    name: String,
    picture: Arc<RenderImage>,
    /// The file as the server holds it: what Copy puts on the clipboard and
    /// Save writes, so neither is a re-encoding of what was uploaded.
    bytes: Arc<Vec<u8>>,
}

/// Shows an image over the whole window, dimmed.
///
/// Dismissed by Escape, by the close button, or by a click on the dimmed area —
/// but not by a click on the picture, which is where you click to look at it.
pub fn show(
    ui: &Rc<Ui>,
    file: &FileInfo,
    picture: Arc<RenderImage>,
    bytes: Vec<u8>,
    cx: &mut App,
) {
    let previous = ui.lightbox.borrow_mut().replace(Open {
        name: file.name.clone(),
        picture,
        bytes: Arc::new(bytes),
    });
    // A full-size photograph is tens of megabytes decoded; the one it
    // replaces is not kept around.
    if let Some(previous) = previous {
        cx.drop_image(previous.picture, None);
    }
    cx.refresh_windows();
}

/// Puts the lightbox away, whichever of its two uses it was in. Answers
/// whether there was anything to dismiss, so Escape can fall through to
/// whatever else it means when there was not.
pub fn dismiss(ui: &Rc<Ui>, cx: &mut App) -> bool {
    let image = ui.lightbox.borrow_mut().take();
    let video = ui.expanded_player();
    if let Some(open) = &image {
        cx.drop_image(open.picture.clone(), None);
    }
    if let Some((_, player)) = &video {
        player.collapse();
    }
    let dismissed = image.is_some() || video.is_some();
    if dismissed {
        cx.refresh_windows();
    }
    dismissed
}

/// The clipboard format for a file, by what its first bytes say it is. The
/// name is not trusted: a `.jpg` that is really a PNG is common.
fn format_of(bytes: &[u8]) -> Option<ImageFormat> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Png => Some(ImageFormat::Png),
        image::ImageFormat::Jpeg => Some(ImageFormat::Jpeg),
        image::ImageFormat::WebP => Some(ImageFormat::Webp),
        image::ImageFormat::Gif => Some(ImageFormat::Gif),
        image::ImageFormat::Bmp => Some(ImageFormat::Bmp),
        _ => None,
    }
}

fn copy(ui: &Rc<Ui>, cx: &mut App) {
    let Some(bytes) = ui.lightbox.borrow().as_ref().map(|open| open.bytes.clone()) else {
        return;
    };
    match format_of(&bytes) {
        Some(format) => {
            cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                format,
                bytes.as_ref().clone(),
            )));
            ui.toast("Image copied.", cx);
        }
        None => ui.toast("That image is in a format the clipboard does not take.", cx),
    }
}

fn save(ui: &Rc<Ui>, cx: &mut App) {
    let Some((name, bytes)) = ui
        .lightbox
        .borrow()
        .as_ref()
        .map(|open| (open.name.clone(), open.bytes.clone()))
    else {
        return;
    };
    let directory = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let chosen = cx.prompt_for_new_path(&directory, Some(&name));
    let ui = ui.clone();
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(path))) = chosen.await else {
            return;
        };
        cx.update(|_| {
            runtime::spawn(
                async move { tokio::fs::write(&path, bytes.as_ref()).await },
                move |result, cx| match result {
                    Ok(()) => ui.toast("Saved.", cx),
                    Err(e) => ui.toast(&format!("Could not save it: {e}"), cx),
                },
            );
        });
    })
    .detach();
}

/// Draws the lightbox over everything, or nothing when it is not open.
pub fn render(ui: &Rc<Ui>, cx: &App) -> Option<AnyElement> {
    let image = ui
        .lightbox
        .borrow()
        .as_ref()
        .map(|open| open.picture.clone());
    let video = ui
        .expanded_player()
        .and_then(|(_, player)| player.expanded_frame());
    let is_image = image.is_some();
    let picture = image.or(video)?;

    let mut buttons = h_flex().absolute().top_3().right_3().gap_1();
    if is_image {
        buttons = buttons
            .child(
                kit::icon_button("copy-image", Lucide::Copy, "Copy")
                    .on_click(ui.click(|ui, cx| copy(ui, cx))),
            )
            .child(
                kit::icon_button("save-image", Lucide::Download, "Save")
                    .on_click(ui.click(|ui, cx| save(ui, cx))),
            );
    }
    buttons = buttons.child(
        kit::icon_button("close-lightbox", Lucide::X, "Close").on_click(ui.click(|ui, cx| {
            dismiss(ui, cx);
        })),
    );

    Some(
        div()
            .id("lightbox")
            .absolute()
            .inset_0()
            // Deliberately not a themed colour: it covers the whole window and
            // its one job is to drop everything behind the picture out of the
            // way.
            .bg(gpui_kit::black().opacity(0.8))
            .occlude()
            // Inset, so there is always a strip of backdrop to click on even
            // when the picture fills the window.
            .p_6()
            .on_click(ui.click(|ui, cx| {
                dismiss(ui, cx);
            }))
            .child(
                div()
                    .id("lightbox-picture")
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(picture)
                            .max_w_full()
                            .max_h_full()
                            .object_fit(ObjectFit::Contain),
                    ),
            )
            .child(
                div()
                    .rounded_md()
                    .bg(cx.theme().background.opacity(0.9))
                    .child(buttons),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_is_read_from_the_bytes_not_the_name() {
        assert_eq!(
            format_of(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0]),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            format_of(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F']),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(format_of(b"GIF89a\x01\x00\x01\x00"), Some(ImageFormat::Gif));
        // Not an image at all.
        assert_eq!(format_of(b"hello, world"), None);
        assert_eq!(format_of(&[]), None);
    }
}
