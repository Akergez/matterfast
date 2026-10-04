/// The overflow menu's entries. One enum rather than one callback each: they
/// all travel the same path to the action loop, and the row does not care what
/// any of them mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostAction {
    Edit,
    Delete,
    Pin,
    Unpin,
    Save,
    Unsave,
    MarkUnread,
    CopyLink,
    CopyText,
    /// Ask the LLM agent to summarise this thread.
    Summarise,
    /// Send this message on to another channel.
    Forward,
    /// Have the server DM you about this later.
    Remind,
    /// Show what this message said before it was edited.
    History,
    /// Move this thread to another channel.
    MoveThread,
    /// Confirm you have read a priority message, or take it back.
    Acknowledge,
    Unacknowledge,
}
