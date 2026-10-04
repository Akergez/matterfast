use std::rc::Rc;

use chrono::{Days, Local};
use gpui_kit::App;

use super::forms::{form, Field, FormSpec};
use super::moments::{local_millis, morning_in, next_monday_morning};
use crate::ui::Ui;

/// Schedule a message: answers a Unix millisecond timestamp.
pub fn schedule_message(ui: &Rc<Ui>, cx: &mut App, on_schedule: impl Fn(i64, &mut App) + 'static) {
    let on_schedule = Rc::new(on_schedule);
    // The usual answer, filled in: tomorrow at nine. Typing a date is quicker
    // than paging a calendar to it, and both fields say what they expect.
    let tomorrow = Local::now().date_naive() + Days::new(1);
    let mut spec = FormSpec::new("Schedule message", "Schedule");
    spec.note = "The server keeps it and posts it for you, even with this app closed.".to_string();
    form(
        ui,
        cx,
        spec,
        vec![
            Field::choice(
                "When",
                &[
                    ("tomorrow", "Tomorrow morning"),
                    ("monday", "Monday morning"),
                    ("custom", "The date and time below"),
                ],
                "tomorrow",
            ),
            Field::text("Date (YYYY-MM-DD)", &tomorrow.format("%Y-%m-%d").to_string()),
            Field::text("Time (HH:MM)", "09:00"),
        ],
        move |values, cx| {
            let millis = match values[0].text().as_str() {
                "tomorrow" => morning_in(1),
                "monday" => next_monday_morning(),
                _ => local_millis(&values[1].text(), &values[2].text()),
            };
            // A date that does not read as one keeps the dialog open.
            let Some(millis) = millis else { return false };
            let on_schedule = on_schedule.clone();
            cx.defer(move |cx| on_schedule(millis, cx));
            true
        },
    );
}
