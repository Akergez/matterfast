use std::rc::Rc;

use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, AnyElement, App, ObjectFit};

use super::inline_gif::plays_inline;
use crate::ui::message::rules::{image_size, scaled_size, ImageSize};
use crate::ui::{kit, media, Ui};

/// An attached file. Images show themselves; everything else is a name and a
/// size, which is all there is to say about it without opening it.
pub(super) fn attachment(
    ui: &Rc<Ui>,
    index: usize,
    file: &mattermost_api::models::FileInfo,
    cx: &App,
) -> AnyElement {
    if file.is_image() {
        // Big enough to actually look at. The other clients go to roughly
        // this, and a thumbnail you have to open to see is a thumbnail that
        // makes you open everything.
        //
        // Both bounds matter: capping height alone turns a wide screenshot
        // into a strip, and capping width alone lets a tall photo run down
        // the page.
        let (width, height) = scaled_size(file.width, file.height);
        let source = image_size(
            file.width,
            file.height,
            ui.scale_factor().ceil() as i32,
            file.has_preview_image,
        );
        let still = || match source {
            ImageSize::Preview => ui.avatars.file_preview(&file.id),
            ImageSize::Thumbnail => ui.avatars.file_thumbnail(&file.id),
        };
        // A GIF is posted to be watched. The server's preview of one is a
        // still, so a small enough GIF is fetched whole and plays in place;
        // until it lands, and for the big ones, the still stands in.
        let picture = if plays_inline(file) {
            ui.avatars.file_animation(&file.id).or_else(still)
        } else {
            still()
        };
        let frame = div()
            .id(("image", index))
            .mt_1()
            .w(px(width as f32))
            .h(px(height as f32))
            .max_w_full()
            .rounded_md()
            .overflow_hidden()
            .cursor_pointer()
            .on_click(ui.click({
                let file = file.clone();
                move |ui, cx| ui.open_image(&file, cx)
            }));
        return match picture {
            Some(picture) => frame
                .child(
                    img(picture)
                        // An animation plays only where it has an id to
                        // keep its place under.
                        .id("picture")
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            // The box is the picture's own size from the start, so the
            // conversation does not jump when it lands.
            None => frame.bg(cx.theme().muted).into_any_element(),
        };
    }

    // Video and audio play in place; see `media`.
    if media::is_playable(file) {
        return media::player(ui, index, file, cx);
    }

    // Everything that is not an image: a name, a size, and a way to get it.
    h_flex()
        .id(("file", index))
        .mt_1()
        .gap_1p5()
        .items_center()
        .text_color(cx.theme().muted_foreground)
        .child(kit::Lucide::Paperclip)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(format!("{}  ·  {}", file.name, file.human_size())),
        )
        .child(
            kit::icon_button("save", kit::Lucide::Download, "Save").on_click(ui.click({
                let file = file.clone();
                move |ui, cx| ui.save_attachment(&file, cx)
            })),
        )
        .into_any_element()
}
