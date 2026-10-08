use super::inbox_row::InboxRow;
use super::target::Target;

/// One line of the inbox.
#[derive(Clone)]
pub(super) enum Entry {
    /// A thread, or a message that is not one yet: something somebody wrote.
    Post(InboxRow),
    /// A conversation followed as a whole, by its channel's id.
    Chat(String),
}

impl Entry {
    /// What this line is a line for, which is what the list is told about
    /// when it changes: two lists with the same keys in the same order have
    /// the same rows in the same places, whatever the rows now say.
    pub(super) fn key(&self) -> String {
        match self {
            Entry::Chat(id) => format!("chat:{id}"),
            Entry::Post(row) => match &row.target {
                Target::Followed { root_id, .. } => format!("thread:{root_id}"),
                Target::Thread { root_id, .. } => format!("reply:{root_id}:{}", row.at),
                Target::Message { post_id, .. } => format!("post:{post_id}"),
            },
        }
    }
}

/// What the inbox lists, the newest first.
#[derive(Default)]
pub(super) struct Inbox {
    pub(super) entries: Vec<Entry>,
    /// Whether the server follows more threads for the reader than are
    /// listed, so that the list ends with the way to ask for them.
    pub(super) more: bool,
}

impl Inbox {
    /// How many rows the list has: the entries, and the row that asks for
    /// older threads where there are some.
    pub(super) fn rows(&self) -> usize {
        self.entries.len() + usize::from(self.more)
    }
}
