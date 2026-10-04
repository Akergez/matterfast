/// A caption as it is shown. A reaction arrives with no words around it, so
/// it reads better as "Anna 👏" than as "Anna: 👏" — the caller passes the
/// colon when it wants one.
pub(super) fn caption_line(who: &str, text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let separator = if who.is_empty() { "" } else { " " };
    format!("{who}{separator}{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caption_names_its_speaker_and_an_empty_one_clears_the_line() {
        assert_eq!(caption_line("Anna:", "hello there"), "Anna: hello there");
        assert_eq!(caption_line("Anna", "👏"), "Anna 👏");
        assert_eq!(caption_line("", "hello"), "hello");
        assert_eq!(caption_line("Anna:", "   "), "");
        assert_eq!(caption_line("", ""), "");
    }
}
