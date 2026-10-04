/// Mattermost channel URLs take lowercase letters, digits, dashes and
/// underscores, so everything else becomes a separator.
pub(crate) fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_url_safe() {
        assert_eq!(slugify("Release Planning"), "release-planning");
        assert_eq!(slugify("  Q3 / 2026 — plans!  "), "q3-2026-plans");
        assert_eq!(slugify("keep_underscores"), "keep_underscores");
        // Nothing ASCII to work with: the caller has to notice and ask again.
        assert_eq!(slugify("Привет"), "");
    }
}
