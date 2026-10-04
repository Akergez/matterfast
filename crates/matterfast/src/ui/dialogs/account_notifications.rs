use std::rc::Rc;

use gpui_kit::App;

use super::forms::{form, Field, FormSpec};
use super::levels::ACCOUNT_LEVELS;
use crate::ui::Ui;

/// Account-wide notification settings: desktop level, sound on/off, mention
/// keywords (comma separated), and whether first name counts.
pub fn account_notifications(
    ui: &Rc<Ui>,
    cx: &mut App,
    current: (String, bool, String, bool),
    on_save: impl Fn(String, bool, String, bool, &mut App) + 'static,
) {
    let (level, sound, keywords, first_name) = current;
    let on_save = Rc::new(on_save);
    let mut spec = FormSpec::new("Notifications", "Save");
    spec.note = "Keywords are separated by commas and ignore case.".to_string();
    form(
        ui,
        cx,
        spec,
        vec![
            Field::choice("Desktop notifications", &ACCOUNT_LEVELS, &level),
            Field::switch("Notification sound", "", sound),
            Field::text("Keywords that mention me", &keywords),
            Field::switch(
                "My first name",
                "Notify me when someone types it",
                first_name,
            ),
        ],
        move |values, cx| {
            let level = values[0].text();
            let sound = values[1].bool();
            let keywords = values[2].text().trim().to_string();
            let first_name = values[3].bool();
            let on_save = on_save.clone();
            cx.defer(move |cx| on_save(level, sound, keywords, first_name, cx));
            true
        },
    );
}
