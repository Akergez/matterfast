/// What the channel row's own menu can ask for.
#[derive(Debug, Clone)]
pub enum RowAction {
    MarkRead,
    MarkUnread,
    SetMuted(bool),
    MoveTo(String),
    RenameCategory,
    DeleteCategory,
}
