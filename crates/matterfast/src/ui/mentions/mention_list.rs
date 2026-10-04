use crate::ui::autocomplete::Candidate;
use crate::ui::constants::{COMPLETIONS, GROUP_COMPLETIONS};

/// One mention list out of the people and the groups that matched: people
/// first, and room left for groups whenever there are any.
pub(crate) fn mention_list(mut people: Vec<Candidate>, mut groups: Vec<Candidate>) -> Vec<Candidate> {
    let room = COMPLETIONS.saturating_sub(people.len());
    groups.truncate(room.max(GROUP_COMPLETIONS));
    people.truncate(COMPLETIONS - groups.len());
    people.extend(groups);
    people
}
