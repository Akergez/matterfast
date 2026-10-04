/// How a shortcode should be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rendered {
    /// A Unicode emoji, ready to put in a label.
    Unicode(&'static str),
    /// Nothing standard matched — the server may host it as a custom image.
    Custom,
}
