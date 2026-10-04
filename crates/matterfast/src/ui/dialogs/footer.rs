use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::prelude::*;
use gpui_kit::SharedString;

/// A footer with Cancel and one confirming button, which is what nearly every
/// dialog here ends in.
pub(crate) fn footer(ok: impl Into<SharedString>) -> DialogFooter {
    DialogFooter::new()
        .child(DialogClose::new().child(Button::new("cancel").label("Cancel")))
        .child(DialogAction::new().child(Button::new("ok").label(ok).primary()))
}
