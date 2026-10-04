use serde_json::Value;

/// Whether a parsed theme file is Zed's rather than the toolkit's: a Zed
/// theme says `appearance` and keeps its colours under `style`.
pub fn is_zed(document: &Value) -> bool {
    document["themes"].as_array().is_some_and(|themes| {
        themes
            .iter()
            .any(|theme| theme.get("style").is_some() || theme.get("appearance").is_some())
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::parse::parse;
    use crate::test_support::SAMPLE;

    #[test]
    fn the_two_formats_are_told_apart() {
        assert!(is_zed(&parse(SAMPLE).unwrap()));
        assert!(!is_zed(&json!({ "themes": [{ "name": "N", "mode": "dark", "colors": {} }] })));
        assert!(!is_zed(&json!({ "name": "nothing" })));
    }
}
