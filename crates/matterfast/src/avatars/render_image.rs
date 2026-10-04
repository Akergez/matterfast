use gpui_kit::RenderImage;

/// Wraps decoded pixels for the renderer, which wants them blue-first.
pub fn render_image(mut pixels: image::RgbaImage) -> RenderImage {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    RenderImage::new(vec![image::Frame::new(pixels)])
}
