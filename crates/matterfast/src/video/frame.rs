/// One decoded picture, in the only form both halves agree on: BGRA, which is
/// what the renderer uploads without another pass over the pixels.
pub struct Frame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
}
