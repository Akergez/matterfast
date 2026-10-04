/// The size to draw an attached image at: its own proportions, fitted inside
/// a box big enough to see and small enough to scroll past.
///
/// A file with no dimensions — some servers omit them — gets the full box and
/// `Contain` sorts it out once the picture arrives.
pub fn scaled_size(width: i32, height: i32) -> (i32, i32) {
    const MAX_WIDTH: f64 = 500.0;
    const MAX_HEIGHT: f64 = 350.0;

    if width <= 0 || height <= 0 {
        return (MAX_WIDTH as i32, MAX_HEIGHT as i32);
    }
    let scale = (MAX_WIDTH / width as f64)
        .min(MAX_HEIGHT / height as f64)
        // Never enlarge: a 64px sticker blown up to 420 is a blurry sticker.
        .min(1.0);
    (
        ((width as f64 * scale).round() as i32).max(1),
        ((height as f64 * scale).round() as i32).max(1),
    )
}

/// Server-side sizes available for an attached image, short of downloading
/// the original: a 120×100 thumbnail and a preview capped at 1920px wide
/// (`imageThumbnailWidth` / `imagePreviewWidth` in the Mattermost server,
/// `server/channels/app/file.go`). The thumbnail is what this app used to
/// draw every inline image at, which is why they came out blurry — it is
/// smaller than the box they were shown in even at 1x.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSize {
    Thumbnail,
    Preview,
}

/// The physical box a 120×100 thumbnail is generated into, whatever the
/// source photo's own proportions.
const THUMBNAIL_WIDTH: i32 = 120;
const THUMBNAIL_HEIGHT: i32 = 100;

/// Which server-generated size to fetch for an inline image.
///
/// `scale_factor` is the window's, rounded up: on a 2x display, the
/// logical box the image is drawn into (see [`scaled_size`]) needs roughly
/// twice the source pixels to look sharp, so it is the *physical* size that
/// decides whether the thumbnail is still big enough — not the logical one.
pub fn image_size(width: i32, height: i32, scale_factor: i32, has_preview: bool) -> ImageSize {
    if !has_preview {
        // No preview was generated for this file (the server makes one for
        // almost everything, but not, say, a format it does not decode) —
        // the thumbnail is the only size short of the original, and
        // fetching the original for every such image would defeat the point
        // of a thumbnail at all.
        return ImageSize::Thumbnail;
    }
    let (logical_w, logical_h) = scaled_size(width, height);
    let scale = scale_factor.max(1);
    if logical_w.saturating_mul(scale) <= THUMBNAIL_WIDTH
        && logical_h.saturating_mul(scale) <= THUMBNAIL_HEIGHT
    {
        ImageSize::Thumbnail
    } else {
        ImageSize::Preview
    }
}

#[cfg(test)]
mod tests {
    use super::{image_size, scaled_size, ImageSize};

    #[test]
    fn fits_the_box_without_enlarging() {
        // A tall photo is bounded by height, a wide one by width.
        assert_eq!(scaled_size(3000, 4000), (263, 350));
        assert_eq!(scaled_size(4000, 1000), (500, 125));
        // Smaller than the box: left alone.
        assert_eq!(scaled_size(64, 64), (64, 64));
        // Unknown: the full box, and Contain sorts it out.
        assert_eq!(scaled_size(0, 0), (500, 350));
    }

    #[test]
    fn a_small_sticker_stays_on_the_thumbnail_at_1x() {
        // 64x64 fits inside the 120x100 thumbnail box with room to spare.
        assert_eq!(image_size(64, 64, 1, true), ImageSize::Thumbnail);
    }

    #[test]
    fn the_same_sticker_needs_the_preview_at_2x() {
        // 64x64 doubled is 128x128, past the thumbnail's own 120x100 box.
        assert_eq!(image_size(64, 64, 2, true), ImageSize::Preview);
    }

    #[test]
    fn an_ordinary_photo_always_wants_the_preview() {
        // Clamped to 467x350 by `scaled_size`, already past the thumbnail.
        assert_eq!(image_size(1600, 1200, 1, true), ImageSize::Preview);
    }

    #[test]
    fn no_preview_on_the_server_falls_back_to_the_thumbnail_regardless_of_size() {
        assert_eq!(image_size(1600, 1200, 2, false), ImageSize::Thumbnail);
    }
}
