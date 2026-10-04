use std::path::PathBuf;

/// The rule itself, apart from the environment so it can be tested: a set,
/// non-empty variable wins, and anything else falls back under the home
/// directory. GLib takes the variable as it is given, so this does too.
pub(crate) fn resolve_from(
    variable: Option<PathBuf>,
    home: Option<PathBuf>,
    fallback: &str,
) -> PathBuf {
    match variable {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => home.unwrap_or_else(|| PathBuf::from("/")).join(fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_variable_and_falls_back_under_home() {
        let home = Some(PathBuf::from("/home/someone"));
        assert_eq!(
            resolve_from(Some("/elsewhere/config".into()), home.clone(), ".config"),
            PathBuf::from("/elsewhere/config"),
        );
        // Unset and empty mean the same thing, as the XDG spec says.
        assert_eq!(
            resolve_from(None, home.clone(), ".config"),
            PathBuf::from("/home/someone/.config"),
        );
        assert_eq!(
            resolve_from(Some(PathBuf::new()), home.clone(), ".cache"),
            PathBuf::from("/home/someone/.cache"),
        );
        assert_eq!(
            resolve_from(None, home, ".local/share"),
            PathBuf::from("/home/someone/.local/share"),
        );
    }
}
