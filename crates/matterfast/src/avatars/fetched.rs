use gpui_kit::RenderImage;

/// What a fetch brings back to the main thread.
pub(super) enum Fetched {
    Picture(RenderImage),
    /// Raw bytes to keep, not an image to decode.
    Head(Vec<u8>),
    /// Fetched, but not a picture.
    Undecodable(String),
}
