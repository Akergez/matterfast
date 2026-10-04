use gpui_kit::{px, size, Pixels};

/// `MATTERFAST_SIZE=400x800` opens at a phone-sized window, which is the only
/// practical way to look at the collapsed layout without a phone.
pub(super) fn initial_size() -> gpui_kit::Size<Pixels> {
    let (width, height) = std::env::var("MATTERFAST_SIZE")
        .ok()
        .and_then(|size| parse_size(&size))
        .unwrap_or((1320.0, 840.0));
    size(px(width), px(height))
}

fn parse_size(text: &str) -> Option<(f32, f32)> {
    let (width, height) = text.split_once('x')?;
    let (width, height): (f32, f32) = (width.trim().parse().ok()?, height.trim().parse().ok()?);
    (width > 0.0 && height > 0.0).then_some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_two_positive_numbers() {
        assert_eq!(parse_size("400x800"), Some((400.0, 800.0)));
        assert_eq!(parse_size(" 1320 x 840 "), Some((1320.0, 840.0)));
        assert_eq!(parse_size("400"), None);
        assert_eq!(parse_size("wide x tall"), None);
        assert_eq!(parse_size("0x800"), None);
    }
}
