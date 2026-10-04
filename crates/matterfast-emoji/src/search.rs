use std::collections::BTreeSet;

use crate::found::Found;

/// The emoji whose names contain `term`, best first, at most `limit`.
///
/// Names that *start* with the term come before names that merely contain
/// it — typing ":ta" wants "tada", not "star_struck" — and that ranking
/// happens before the cut, or the cut keeps whatever the table lists first.
/// Within a rank the server's own come first: they are the ones a team made
/// because it wanted them.
pub fn search(term: &str, custom: &BTreeSet<String>, limit: usize) -> Vec<Found> {
    let term = term.trim().trim_matches(':').to_lowercase();
    let rank = |name: &str| {
        if term.is_empty() || name == term {
            Some(0)
        } else if name.starts_with(&term) {
            Some(1)
        } else if name.contains(&term) {
            Some(2)
        } else {
            None
        }
    };
    let mut found: Vec<(u8, Found)> = custom
        .iter()
        .filter_map(|name| Some((rank(name)?, Found::Custom(name.clone()))))
        .chain(emojis::iter().filter_map(|emoji| {
            let name = emoji.shortcode()?;
            Some((rank(name)?, Found::Unicode(name, emoji.as_str())))
        }))
        .collect();
    // Stable, so each rank keeps the order it was listed in.
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().take(limit).map(|(_, found)| found).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_ranks_before_it_cuts() {
        let custom: BTreeSet<String> = ["shipit", "tableflip"]
            .into_iter()
            .map(String::from)
            .collect();
        let names = |term: &str, limit: usize| -> Vec<String> {
            search(term, &custom, limit)
                .iter()
                .map(|found| found.name().to_string())
                .collect()
        };
        // "star_struck" contains "ta" and sits earlier in the table than
        // "tada"; a prefix still wins, and the server's own lead their rank.
        let found = names("ta", 8);
        assert_eq!(found[0], "tableflip");
        assert!(found.iter().all(|name| name.starts_with("ta")), "{found:?}");
        assert_eq!(names("tada", 3)[0], "tada");
        assert_eq!(names(":SHIPI", 5), ["shipit"]);
        assert!(names("no-such-emoji-anywhere", 5).is_empty());
        assert_eq!(names("", 120).len(), 120);
        assert_eq!(names("", 1), ["shipit"]);
    }
}
