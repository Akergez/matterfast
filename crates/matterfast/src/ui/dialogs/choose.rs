use std::rc::Rc;

use gpui_adaptive::{Ask, Command};
use gpui_kit::App;

use super::callback::{run_later, Callback};
use crate::ui::Ui;

/// Offers a handful of answers as buttons, one of which is picked.
pub(crate) fn choose(
    ui: &Rc<Ui>,
    cx: &mut App,
    title: &str,
    body: &str,
    options: Vec<(&'static str, Callback)>,
) {
    let answers = options.into_iter().enumerate().map(|(index, (label, action))| {
        Command::new(("option", index), label).on_trigger(move |_, cx| run_later(&action, cx))
    });
    let question = answers.fold(Ask::new("choose", title.to_string()).detail(body.to_string()), Ask::answer);
    ui.with_window(cx, move |window, cx| question.open(window, cx));
}
