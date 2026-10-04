use std::ops::Range;

/// What has to change in a list of `old` rows to make it `new`: the range to
/// replace and how many rows go in its place. `None` when nothing does.
///
/// The common prefix and the common suffix are left alone, which is the whole
/// point: an older page arriving is a splice at the top, a new message one at
/// the bottom, and in both cases every row the reader is looking at keeps its
/// identity and the list keeps its anchor.
pub(crate) fn splice_plan(old: &[String], new: &[String]) -> Option<(Range<usize>, usize)> {
    let mut prefix = 0;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old.len() - prefix
        && suffix < new.len() - prefix
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let removed = prefix..old.len() - suffix;
    let added = new.len() - suffix - prefix;
    (!removed.is_empty() || added > 0).then_some((removed, added))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_message_is_a_splice_at_the_bottom() {
        let before: Vec<String> = ["day", "a", "b"].map(String::from).into();
        let after: Vec<String> = ["day", "a", "b", "c"].map(String::from).into();
        assert_eq!(splice_plan(&before, &after), Some((3..3, 1)));
    }

    #[test]
    fn nothing_changing_is_no_splice() {
        let rows: Vec<String> = ["day", "a"].map(String::from).into();
        assert_eq!(splice_plan(&rows, &rows), None);
    }

    #[test]
    fn the_echo_of_a_sent_message_replaces_only_its_own_row() {
        let before = vec!["day:Today".to_string(), "post:a".into(), "post:pending1".into()];
        let after = vec!["day:Today".to_string(), "post:a".into(), "post:real".into()];
        assert_eq!(splice_plan(&before, &after), Some((2..3, 1)));
        // And a delete takes one row out.
        assert_eq!(splice_plan(&after, &after[..2].to_vec()), Some((2..3, 0)));
    }
}
