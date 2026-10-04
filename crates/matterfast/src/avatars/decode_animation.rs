use gpui_kit::RenderImage;

use super::constants::{MAX_ANIMATION_BYTES, MAX_DECODED};

/// Every frame of an animated GIF, or `None` for anything that is not one —
/// another format, a GIF with a single frame, or one too big to hold.
pub(super) fn decode_animation(bytes: &[u8], big: bool) -> Option<RenderImage> {
    use image::AnimationDecoder;

    if !bytes.starts_with(b"GIF8") {
        return None;
    }
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let mut frames = Vec::new();
    let mut held = 0usize;
    for frame in decoder.into_frames() {
        // A file cut short still has the frames before the cut.
        let Ok(frame) = frame else { break };
        let delay = frame.delay();
        let mut pixels = frame.into_buffer();
        if big && (pixels.width() > MAX_DECODED || pixels.height() > MAX_DECODED) {
            pixels = image::DynamicImage::ImageRgba8(pixels)
                .resize(MAX_DECODED, MAX_DECODED, image::imageops::FilterType::Triangle)
                .into_rgba8();
        }
        held += pixels.len();
        if held > MAX_ANIMATION_BYTES {
            return None;
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        frames.push(image::Frame::from_parts(pixels, 0, 0, delay));
    }
    (frames.len() > 1).then(|| RenderImage::new(frames))
}
