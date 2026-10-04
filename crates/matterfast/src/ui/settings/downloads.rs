/// A download count the way a list row has room for it.
pub(super) fn downloads(count: u64) -> String {
    match count {
        0..1_000 => count.to_string(),
        1_000..1_000_000 => format!("{}K", count / 1_000),
        _ => format!("{:.1}M", count as f64 / 1_000_000.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_count_is_short_enough_for_a_row() {
        assert_eq!(downloads(0), "0");
        assert_eq!(downloads(812), "812");
        assert_eq!(downloads(1_000), "1K");
        assert_eq!(downloads(496_067), "496K");
        assert_eq!(downloads(1_163_124), "1.2M");
    }
}
