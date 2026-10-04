use std::rc::Rc;

use gpui_kit::App;

use super::forms::{form, Field, FormSpec};
use crate::ui::Ui;

/// Rename a channel, or change its topic. Answers (display_name, header).
pub fn edit_channel(
    ui: &Rc<Ui>,
    cx: &mut App,
    current: (String, String),
    on_save: impl Fn(String, String, &mut App) + 'static,
) {
    let on_save = Rc::new(on_save);
    form(
        ui,
        cx,
        FormSpec::new("Channel details", "Save"),
        vec![
            Field::text("Name", &current.0),
            Field::text("Topic", &current.1),
        ],
        move |values, cx| {
            let name = values[0].text().trim().to_string();
            let header = values[1].text();
            let on_save = on_save.clone();
            cx.defer(move |cx| on_save(name, header, cx));
            true
        },
    );
}
