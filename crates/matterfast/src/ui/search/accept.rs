use super::suggestion::Suggestion;
use super::word_start::word_start;

/// Writes a picked row over the word under the cursor. Answers the new text
/// and where the cursor belongs in it.
pub(super) fn accept(text: &str, cursor: usize, picked: &Suggestion) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let Some(before) = text.get(..cursor) else {
        return (text.to_string(), cursor);
    };
    let start = word_start(before);
    let mut out = String::with_capacity(text.len() + picked.insert.len() + 1);
    out.push_str(&text[..start]);
    out.push_str(&picked.insert);
    if !picked.open_ended {
        out.push(' ');
    }
    let caret = out.len();
    out.push_str(&text[cursor..]);
    (out, caret)
}

#[cfg(test)]
mod tests {
    use super::super::modifiers::modifiers;
    use super::super::people::person;
    use super::*;

    #[test]
    fn a_modifier_is_written_without_a_space_and_a_value_with_one() {
        let modifier = &modifiers("fr", true)[0];
        assert_eq!(accept("fr", 2, modifier), ("from:".to_string(), 5));
        let anna = person("anna", "Anna Berg");
        assert_eq!(
            accept("release from:an notes", 15, &anna),
            ("release from:anna  notes".to_string(), 18),
        );
    }
}
