/// What the word under the cursor is asking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hint {
    /// The beginning of a word that may become a modifier; `first` when it
    /// is the first word of the line.
    Modifiers { typed: String, first: bool },
    /// After `from:` — whose messages.
    From(String),
    /// After `in:` — which channel's.
    In(String),
    /// After a date modifier, which is carried along so the row can say it.
    Date(&'static str, String),
}
