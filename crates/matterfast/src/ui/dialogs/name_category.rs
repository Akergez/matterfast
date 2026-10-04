use std::rc::Rc;

use gpui_kit::App;

use super::forms::{form, Field, FormSpec};
use crate::ui::Ui;

/// Name a category, whether new or being renamed.
pub fn name_category(
    ui: &Rc<Ui>,
    cx: &mut App,
    heading: &str,
    current: &str,
    on_save: impl Fn(String, &mut App) + 'static,
) {
    let on_save = Rc::new(on_save);
    form(
        ui,
        cx,
        FormSpec::new(heading, "Save"),
        vec![Field::text("Name", current)],
        move |values, cx| {
            let name = values[0].text().trim().to_string();
            if name.is_empty() {
                return false;
            }
            let on_save = on_save.clone();
            cx.defer(move |cx| on_save(name, cx));
            true
        },
    );
}
