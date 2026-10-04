/// How a mention is written into the Markdown.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sigil {
    /// "@Anna Petrova" — an ordinary message.
    Keep,
    /// "Anna Petrova" — a system line, which reads "Anna joined the channel".
    Drop,
}
