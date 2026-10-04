/// Desktop notification levels for a single channel, in menu order. The ids
/// are Mattermost's own; `"default"` means "whatever the account says".
pub(crate) const CHANNEL_LEVELS: [(&str, &str); 4] = [
    ("default", "Global default"),
    ("all", "All new messages"),
    ("mention", "Mentions only"),
    ("none", "Nothing"),
];

/// The same list for the account, which has nothing to fall back to.
pub(crate) const ACCOUNT_LEVELS: [(&str, &str); 3] = [
    ("all", "All new messages"),
    ("mention", "Mentions only"),
    ("none", "Nothing"),
];

/// Where `value` sits in `levels`, falling back to the first entry: the server
/// can send a level we do not offer, and there is nothing better to show.
pub(crate) fn level_index(levels: &[(&str, &str)], value: &str) -> usize {
    levels.iter().position(|(id, _)| *id == value).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_levels_fall_back_to_the_first() {
        assert_eq!(level_index(&CHANNEL_LEVELS, "mention"), 2);
        assert_eq!(level_index(&ACCOUNT_LEVELS, "mention"), 1);
        assert_eq!(level_index(&CHANNEL_LEVELS, "something_new"), 0);
    }
}
