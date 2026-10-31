use std::rc::Rc;

use gpui_adaptive::{Command, Confirm};
use gpui_kit::{App, SharedString};

use super::callback::{run_later, Callback};
use crate::ui::Ui;

/// Asks a yes-or-no question. `danger` paints the confirming button as
/// something that cannot be taken back, and then Enter does not confirm.
pub fn confirm(
    ui: &Rc<Ui>,
    cx: &mut App,
    title: &str,
    body: &str,
    ok: &str,
    danger: bool,
    on_ok: impl Fn(&mut App) + 'static,
) {
    let title: SharedString = title.to_string().into();
    let on_ok: Callback = Rc::new(on_ok);
    let command = Command::new("ok", ok.to_string()).on_trigger(move |_, cx| run_later(&on_ok, cx));
    let command = if danger { command.destructive() } else { command.primary() };
    let question = Confirm::new("confirm", title).body(body.to_string()).confirm(command);
    ui.with_window(cx, move |window, cx| question.open(window, cx));
}
