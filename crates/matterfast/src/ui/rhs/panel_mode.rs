#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelMode {
    Hidden,
    /// Viewing the thread rooted at this post id.
    Thread(String),
    /// Results for a search, held so a redraw does not lose them.
    Search(String),
}
