use serde_json::Value;

use crate::error::Error;
use crate::relax::relax;

/// A theme file as text, parsed the way Zed parses one: comments and trailing
/// commas are allowed, and published themes do have them.
pub fn parse(source: &str) -> Result<Value, Error> {
    Ok(serde_json::from_str(&relax(source))?)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn comments_and_trailing_commas_are_read_past() {
        let document = parse(
            "{\n  // a family\n  \"name\": \"C\", /* inline */\n  \"url\": \"https://a/b//c\",\n  \
             \"note\": \"a \\\" /* not a comment */ , ]\",\n  \"themes\": [1, 2, ],\n}",
        )
        .unwrap();
        assert_eq!(document["url"], "https://a/b//c");
        assert_eq!(document["note"], "a \" /* not a comment */ , ]");
        assert_eq!(document["themes"], json!([1, 2]));
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(matches!(parse("{"), Err(Error::Json(_))));
    }
}
