use crate::ui::dialogs::callback::Callback;
use crate::ui::kit::Lucide;

/// Something a row offers at its right-hand end.
pub(super) enum Trailing {
    /// Words: "Joined", "Admin".
    Label(String),
    /// A button that acknowledges its own click, because the caller's answer
    /// arrives over the network much later.
    Button {
        label: String,
        done: String,
        action: Callback,
    },
    Icon {
        icon: Lucide,
        tooltip: String,
        action: Callback,
    },
}
