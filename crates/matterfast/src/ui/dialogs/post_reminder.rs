use std::rc::Rc;

use gpui_kit::App;

use super::callback::Callback;
use super::choose::choose;
use super::moments::{from_now, morning_in};
use crate::ui::Ui;

/// Set a reminder about a post: answers a Unix millisecond timestamp.
pub fn post_reminder(ui: &Rc<Ui>, cx: &mut App, on_remind: impl Fn(i64, &mut App) + 'static) {
    let on_remind = Rc::new(on_remind);
    let option = |when: fn() -> Option<i64>| -> Callback {
        let on_remind = on_remind.clone();
        Rc::new(move |cx| {
            // A reminder we cannot place is simply not set.
            if let Some(millis) = when() {
                on_remind(millis, cx);
            }
        })
    };
    choose(
        ui,
        cx,
        "Remind me about this",
        "The system bot will send you the message again.",
        vec![
            ("In 30 minutes", option(|| Some(from_now(30)))),
            ("In 1 hour", option(|| Some(from_now(60)))),
            ("In 2 hours", option(|| Some(from_now(120)))),
            ("Tomorrow morning", option(|| morning_in(1))),
        ],
    );
}
