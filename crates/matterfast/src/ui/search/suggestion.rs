/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// What replaces the word under the cursor.
    pub insert: String,
    pub label: String,
    /// Dimmed, to the right.
    pub detail: String,
    /// A modifier is half a word: the cursor stays right behind it, and the
    /// list goes on to what can follow.
    pub open_ended: bool,
}
