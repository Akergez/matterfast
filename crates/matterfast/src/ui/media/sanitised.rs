/// A file id or an extension as part of a file name: only what cannot mean
/// anything to a shell or a path.
pub(super) fn sanitised(text: &str) -> String {
    let clean: String = text
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(32)
        .collect();
    if clean.is_empty() {
        "bin".to_string()
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_name_part_is_only_letters_and_digits() {
        assert_eq!(sanitised("abc123"), "abc123");
        assert_eq!(sanitised("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitised(""), "bin");
        assert_eq!(sanitised("$(rm -rf)"), "rmrf");
    }
}
