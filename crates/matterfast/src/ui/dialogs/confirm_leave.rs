use std::rc::Rc;

use gpui_kit::App;

use super::confirm::confirm;
use crate::ui::Ui;

/// Confirm leaving a channel.
pub fn confirm_leave(
    ui: &Rc<Ui>,
    cx: &mut App,
    channel_name: &str,
    on_leave: impl Fn(&mut App) + 'static,
) {
    confirm(
        ui,
        cx,
        &format!("Leave {channel_name}?"),
        "You will stop receiving its messages. You can join a public channel again later.",
        "Leave",
        true,
        on_leave,
    );
}
