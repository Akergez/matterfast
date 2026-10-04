use std::rc::Rc;

use gpui_kit::App;

use super::confirm::confirm;
use crate::ui::Ui;

/// Confirm archiving a channel. Archiving hides it for everyone, so it asks
/// in the same words the other clients use.
pub fn confirm_archive(
    ui: &Rc<Ui>,
    cx: &mut App,
    channel_name: &str,
    on_archive: impl Fn(&mut App) + 'static,
) {
    confirm(
        ui,
        cx,
        "Archive this channel?",
        &format!(
            "{channel_name} will be hidden for everyone. Its messages are kept, and an admin can \
             bring it back."
        ),
        "Archive",
        true,
        on_archive,
    );
}
