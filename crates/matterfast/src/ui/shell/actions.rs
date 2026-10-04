gpui_kit::actions!(
    matterfast,
    [
        Quit,
        QuickSwitch,
        FocusSearch,
        OpenInbox,
        ClosePanel,
        NewChannel,
        NextUnread,
        PreviousUnread,
        OpenSettings
    ]
);

/// The key context the shortcuts live in.
pub(super) const CONTEXT: &str = "Matterfast";
