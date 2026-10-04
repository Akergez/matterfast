use gpui_kit::RenderImage;

use super::frame::Frame;

/// Turns a decoded frame into something the renderer can draw. `None` for a
/// frame whose buffer does not hold what its own header claims.
pub fn frame_image(frame: Frame) -> Option<RenderImage> {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let row = width * 4;
    let pixels = if frame.stride == row {
        let mut pixels = frame.pixels;
        pixels.truncate(row * height);
        pixels
    } else {
        // Padded rows: the renderer wants them packed.
        let mut packed = Vec::with_capacity(row * height);
        for line in frame.pixels.chunks(frame.stride).take(height) {
            packed.extend_from_slice(line.get(..row)?);
        }
        packed
    };
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, pixels)?;
    Some(RenderImage::new(vec![image::Frame::new(buffer)]))
}
