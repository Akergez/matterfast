use gpui_kit::RenderImage;

use super::constants::{ANIMATION_PREFIX, FILE_PREFIX, FILE_PREVIEW_PREFIX, MAX_DECODED};
use super::decode_animation::decode_animation;
use super::render_image::render_image;

/// Decodes to the size it will be drawn at rather than the size it arrived
/// in. Faces and emoji are already small; only the file images are worth
/// scaling. Runs off the main thread.
pub(super) fn decode(id: &str, bytes: &[u8]) -> Result<RenderImage, image::ImageError> {
    let big = id.starts_with(FILE_PREVIEW_PREFIX)
        || id.starts_with(FILE_PREFIX)
        || id.starts_with(ANIMATION_PREFIX);
    if let Some(animation) = decode_animation(bytes, big) {
        return Ok(animation);
    }
    let mut picture = image::load_from_memory(bytes)?;
    if big && (picture.width() > MAX_DECODED || picture.height() > MAX_DECODED) {
        picture = picture.resize(
            MAX_DECODED,
            MAX_DECODED,
            image::imageops::FilterType::Triangle,
        );
    }
    Ok(render_image(picture.into_rgba8()))
}

#[cfg(test)]
mod tests {
    use super::super::texture_bytes::texture_bytes;
    use super::*;

    fn gif(frames: usize, side: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for index in 0..frames {
                let pixels = image::RgbaImage::from_pixel(
                    side,
                    side,
                    image::Rgba([index as u8 * 40, 0, 0, 255]),
                );
                let delay = image::Delay::from_numer_denom_ms(100, 1);
                encoder
                    .encode_frame(image::Frame::from_parts(pixels, 0, 0, delay))
                    .unwrap();
            }
        }
        bytes
    }

    #[test]
    fn an_animated_gif_keeps_every_frame() {
        let animation = decode("emoji:party", &gif(3, 8)).unwrap();
        assert_eq!(animation.frame_count(), 3);
        // And is counted as three pictures' worth of memory, not one.
        assert_eq!(texture_bytes(&animation), 8 * 8 * 4 * 3);
    }

    #[test]
    fn a_single_frame_gif_is_a_still() {
        assert_eq!(decode("emoji:still", &gif(1, 8)).unwrap().frame_count(), 1);
    }

    #[test]
    fn a_big_attachment_animation_is_scaled_down_frame_by_frame() {
        let animation = decode("animation:file", &gif(2, MAX_DECODED + 200)).unwrap();
        assert_eq!(animation.frame_count(), 2);
        assert_eq!(animation.size(1).width.0 as u32, MAX_DECODED);
    }
}
