/// A search as the real server reads one: words that must all be there (or
/// any of them, for an "or" search), and the modifiers that narrow where and
/// when. Enough of the grammar for a client to be tested against: `from:`,
/// `in:`, `before:`, `after:`, `on:` and `-word`.
#[derive(Debug, Default, PartialEq)]
pub(super) struct SearchTerms {
    pub(super) words: Vec<String>,
    pub(super) excluded: Vec<String>,
    pub(super) from: Vec<String>,
    pub(super) channels: Vec<String>,
    /// Days since the Unix epoch, in the asker's time zone.
    pub(super) before: Option<i64>,
    pub(super) after: Option<i64>,
    pub(super) on: Option<i64>,
}
