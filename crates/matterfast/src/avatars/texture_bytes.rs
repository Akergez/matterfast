use gpui_kit::RenderImage;

/// What a decoded texture costs in memory: four bytes a pixel, whatever it
/// was compressed to on the wire.
pub(super) fn texture_bytes(texture: &RenderImage) -> usize {
    let size = texture.size(0);
    // An animation holds every one of its frames.
    (size.width.0.max(0) as usize)
        * (size.height.0.max(0) as usize)
        * 4
        * texture.frame_count().max(1)
}
