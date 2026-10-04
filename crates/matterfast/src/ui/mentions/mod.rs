//! Answering an `@` completion from memory.

mod local_groups;
mod local_mentions;
mod mention_list;
mod mentioned_names;
#[cfg(test)]
mod tests;

pub(super) use local_groups::local_groups;
pub(super) use local_mentions::local_mentions;
pub(super) use mention_list::mention_list;
pub(super) use mentioned_names::mentioned_names;
