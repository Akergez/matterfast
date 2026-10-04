use mattermost_api::models::Group;

use crate::ui::autocomplete::Candidate;
use crate::ui::constants::COMPLETIONS;
use crate::ui::group;

/// The groups an `@` completion offers for `lowered`, the same way people are
/// found: by what is written after the `@` first, then by what the group is
/// called.
pub(crate) fn local_groups(groups: &[Group], lowered: &str) -> Vec<Candidate> {
    let mut found: Vec<(u8, &Group)> = groups
        .iter()
        .filter_map(|group| {
            let name = group.name.to_lowercase();
            let title = group.display_name.to_lowercase();
            let rank = if name.starts_with(lowered) {
                0
            } else if title
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word.starts_with(lowered))
            {
                1
            } else if name.contains(lowered) || title.contains(lowered) {
                2
            } else {
                return None;
            };
            Some((rank, group))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    found
        .into_iter()
        .take(COMPLETIONS)
        .map(|(_, group)| Candidate {
            insert: format!("@{}", group.name),
            primary: group.display_name.clone(),
            secondary: group::summary(group),
            emoji: None,
            image: None,
            user_id: None,
        })
        .collect()
}
