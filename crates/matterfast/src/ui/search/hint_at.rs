use super::constants::{FILES, MODIFIERS};
use super::hint::Hint;
use super::word_start::word_start;

/// What to suggest for the word that ends at `cursor`, a byte offset into
/// `text`. Nothing for an ordinary word: the list is for the grammar, and
/// must stay out of the way of somebody typing what they are looking for.
pub(super) fn hint_at(text: &str, cursor: usize) -> Option<Hint> {
    let before = text.get(..cursor)?;
    let start = word_start(before);
    let word = before[start..].to_lowercase();
    let after = |modifier: &str| word.strip_prefix(modifier).map(str::to_string);

    if let Some(name) = after("from:") {
        return Some(Hint::From(name.trim_start_matches('@').to_string()));
    }
    if let Some(name) = after("in:") {
        return Some(Hint::In(name.trim_start_matches('~').to_string()));
    }
    for modifier in ["before:", "after:", "on:"] {
        if let Some(day) = after(modifier) {
            return Some(Hint::Date(modifier, day));
        }
    }
    let first = before[..start].trim().is_empty();
    let begins = |(name, _): &(&str, &str)| name.starts_with(word.as_str());
    (MODIFIERS.iter().any(begins) || (first && begins(&FILES)))
        .then_some(Hint::Modifiers { typed: word, first })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The usual case: the cursor is at the end of what has been typed.
    fn typed(text: &str) -> Option<Hint> {
        hint_at(text, text.len())
    }

    #[test]
    fn an_empty_box_offers_the_modifiers() {
        assert_eq!(
            typed(""),
            Some(Hint::Modifiers {
                typed: String::new(),
                first: true
            })
        );
    }

    #[test]
    fn a_later_word_is_not_the_first() {
        assert_eq!(
            typed("release "),
            Some(Hint::Modifiers {
                typed: String::new(),
                first: false
            }),
        );
    }

    #[test]
    fn the_beginning_of_a_modifier_is_a_hint() {
        assert_eq!(
            typed("f"),
            Some(Hint::Modifiers {
                typed: "f".into(),
                first: true
            })
        );
    }

    #[test]
    fn an_ordinary_word_is_left_alone() {
        assert_eq!(typed("release"), None);
        assert_eq!(typed("from the start"), None);
        // `file` is the beginning of a modifier only as the first word.
        assert_eq!(typed("notes fil"), None);
    }

    #[test]
    fn a_modifier_asks_for_what_follows_it() {
        assert_eq!(typed("from:"), Some(Hint::From(String::new())));
        assert_eq!(typed("notes From:@An"), Some(Hint::From("an".into())));
        assert_eq!(typed("in:~town"), Some(Hint::In("town".into())));
        assert_eq!(typed("on:2026-1"), Some(Hint::Date("on:", "2026-1".into())));
        assert_eq!(typed("x after:"), Some(Hint::Date("after:", String::new())));
    }

    #[test]
    fn only_the_word_up_to_the_cursor_counts() {
        assert_eq!(
            hint_at("from:anna release", 7),
            Some(Hint::From("an".into()))
        );
    }
}
