use crate::rendered::Rendered;
use crate::resolve::resolve;

/// What to show for a reaction chip: the emoji if we have one, else `:name:`
/// so the reader at least knows which reaction it was.
pub fn label(name: &str) -> String {
    match resolve(name) {
        Rendered::Unicode(e) => e.to_string(),
        Rendered::Custom => format!(":{name}:"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_reactions_resolve_to_unicode() {
        assert_eq!(label("tada"), "🎉");
        assert_eq!(label("eyes"), "👀");
        assert_eq!(label("+1"), "👍");
        assert_eq!(label("-1"), "👎");
        assert_eq!(label("rocket"), "🚀");
    }
}
