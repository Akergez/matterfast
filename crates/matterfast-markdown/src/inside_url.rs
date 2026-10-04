/// Whether the byte at `at` sits inside a word that is a URL.
pub(crate) fn inside_url(text: &str, at: usize) -> bool {
    // By character, not by byte: a no-break space is whitespace too, and it
    // is two bytes wide.
    let start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(index, c)| index + c.len_utf8());
    let end = text[at..]
        .find(char::is_whitespace)
        .map_or(text.len(), |space| at + space);
    text[start..end].contains("://")
}
