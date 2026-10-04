/// A colour from `color`, without its alpha.
pub(crate) fn opaque(color: &str) -> String {
    color.chars().take(7).collect()
}
