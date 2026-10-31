use gpui_adaptive::TextFieldState;
use gpui_kit::Entity;

/// A field as the form holds it while it is open.
pub(super) enum Slot {
    Text {
        label: String,
        input: Entity<TextFieldState>,
    },
    Switch {
        label: String,
        subtitle: String,
        on: bool,
    },
    Choice {
        label: String,
        options: Vec<(String, String)>,
        selected: usize,
    },
}
