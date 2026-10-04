use super::inbox_row::InboxRow;

#[derive(Default)]
pub(super) struct Inbox {
    pub(super) mentions: Vec<InboxRow>,
    pub(super) threads: Vec<InboxRow>,
    pub(super) saved: Vec<InboxRow>,
}
