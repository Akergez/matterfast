use super::constants::{FILES, MODIFIERS};
use super::suggestion::Suggestion;

pub(super) fn modifiers(typed: &str, first: bool) -> Vec<Suggestion> {
    MODIFIERS
        .iter()
        .chain(first.then_some(&FILES))
        .filter(|(name, _)| name.starts_with(typed))
        .map(|(name, purpose)| Suggestion {
            insert: (*name).to_string(),
            label: (*name).to_string(),
            detail: (*purpose).to_string(),
            open_ended: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(rows: &[Suggestion]) -> Vec<&str> {
        rows.iter().map(|row| row.label.as_str()).collect()
    }

    #[test]
    fn an_empty_box_lists_every_modifier() {
        assert_eq!(
            labels(&modifiers("", true)),
            ["from:", "in:", "before:", "after:", "on:", "file:"],
        );
    }

    #[test]
    fn file_is_only_offered_where_it_would_turn_the_line_into_a_file_search() {
        assert!(!labels(&modifiers("", false)).contains(&"file:"));
    }

    #[test]
    fn the_beginning_of_a_modifier_narrows_the_list() {
        assert_eq!(labels(&modifiers("f", true)), ["from:", "file:"]);
        assert_eq!(labels(&modifiers("be", false)), ["before:"]);
    }
}
