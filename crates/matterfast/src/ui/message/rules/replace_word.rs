/// Replaces whole-word occurrences only, so a handle that happens to be a
/// substring of another word is left alone.
pub(crate) fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(word) {
        let before_ok = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after = &rest[at + word.len()..];
        let after_ok = after
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');

        out.push_str(&rest[..at]);
        if before_ok && after_ok {
            out.push_str(with);
        } else {
            out.push_str(word);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::replace_word;

    #[test]
    fn replaces_whole_words_only() {
        assert_eq!(replace_word("sin joined", "sin", "Anna"), "Anna joined");
        // A handle inside another word is not that person.
        assert_eq!(
            replace_word("sinner joined", "sin", "Anna"),
            "sinner joined"
        );
        assert_eq!(replace_word("ask sin.", "sin", "Anna"), "ask Anna.");
        assert_eq!(replace_word("sin_bot left", "sin", "Anna"), "sin_bot left");
    }
}
