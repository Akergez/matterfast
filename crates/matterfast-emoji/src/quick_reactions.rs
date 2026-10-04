/// A short menu of reactions for the quick-react popover, in the order
/// Mattermost itself offers them.
pub const QUICK_REACTIONS: &[&str] = &[
    "+1",
    "-1",
    "smile",
    "tada",
    "eyes",
    "heart",
    "rocket",
    "white_check_mark",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resolve, Rendered};

    #[test]
    fn every_quick_reaction_renders() {
        for name in QUICK_REACTIONS {
            assert!(
                matches!(resolve(name), Rendered::Unicode(_)),
                "{name} should have a Unicode rendering"
            );
        }
    }
}
