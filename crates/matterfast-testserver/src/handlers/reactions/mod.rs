//! Adding and removing a reaction, and telling every client about it.

mod add_reaction;
mod remove_reaction;

pub(crate) use add_reaction::add_reaction;
pub(crate) use remove_reaction::remove_reaction;
