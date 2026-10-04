/// Which list of the inbox is in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum InboxTab {
    #[default]
    Mentions,
    Threads,
    Saved,
}
