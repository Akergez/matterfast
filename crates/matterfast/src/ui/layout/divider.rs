/// Which of the two dividers between the panes: the one after the channel
/// list, or the one before the thread panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    Sidebar,
    Panel,
}

impl Divider {
    pub(super) fn key(self) -> &'static str {
        match self {
            Divider::Sidebar => "sidebar_width",
            Divider::Panel => "panel_width",
        }
    }
}
