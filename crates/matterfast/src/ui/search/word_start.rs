/// Where the word that ends at the end of `before` begins.
pub(super) fn word_start(before: &str) -> usize {
    before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8())
}
