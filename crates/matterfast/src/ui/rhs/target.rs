/// Where an inbox row goes when pressed.
#[derive(Clone)]
pub(super) enum Target {
    /// A reply: its channel, and the thread it is in.
    Thread { channel_id: String, root_id: String },
    /// Not a reply, so there is no thread to open — the useful thing is the
    /// message itself.
    Message { channel_id: String, post_id: String },
    /// A followed thread, by its root, and the channel it is in: the thread
    /// is opened beside its own conversation, not beside whichever one
    /// happened to be on screen.
    Followed { channel_id: String, root_id: String },
}
