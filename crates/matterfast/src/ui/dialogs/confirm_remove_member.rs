use std::rc::Rc;

use gpui_kit::App;

use super::confirm::confirm;
use crate::ui::Ui;

/// Confirm removing someone from a channel.
pub fn confirm_remove_member(
    ui: &Rc<Ui>,
    cx: &mut App,
    name: &str,
    on_remove: impl Fn(&mut App) + 'static,
) {
    confirm(
        ui,
        cx,
        &format!("Remove {name}?"),
        "They will stop receiving its messages. You can add them back later.",
        "Remove",
        true,
        on_remove,
    );
}
