/// The `@names` a message mentions, by the same rule the renderer uses.
pub(crate) fn mentioned_names(message: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = message;
    while let Some(at) = rest.find('@') {
        // Mid-word, so it is an email address rather than a mention.
        let preceded = rest[..at].chars().next_back();
        rest = &rest[at + 1..];
        if preceded.is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '-' | '_')))
            .unwrap_or(rest.len());
        if end > 0 {
            names.push(rest[..end].trim_end_matches('.').to_string());
        }
        rest = &rest[end..];
    }
    names
}
