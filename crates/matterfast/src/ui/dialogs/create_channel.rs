use std::rc::Rc;

use gpui_kit::App;

use super::forms::{form, Field, FormSpec};
use super::slugify::slugify;
use crate::ui::Ui;

/// Create a channel. Answers (display_name, url_name, purpose, private).
pub fn create_channel(
    ui: &Rc<Ui>,
    cx: &mut App,
    on_create: impl Fn(String, String, String, bool, &mut App) + 'static,
) {
    let on_create = Rc::new(on_create);
    let mut spec = FormSpec::new("Create a channel", "Create");
    // The URL follows the name until someone edits it themselves.
    spec.follow = Some(|name| slugify(name));
    form(
        ui,
        cx,
        spec,
        vec![
            Field::text("Name", ""),
            Field::text("URL name", ""),
            Field::text("Purpose (optional)", ""),
            Field::switch("Private channel", "Only invited people can find it", false),
        ],
        move |values, cx| {
            let display_name = values[0].text().trim().to_string();
            // A channel with no name is the one mistake worth blocking
            // outright.
            if display_name.is_empty() {
                return false;
            }
            let mut url_name = values[1].text().trim().to_string();
            // Emptied by hand, or a name with nothing ASCII in it to slug.
            if url_name.is_empty() {
                url_name = slugify(&display_name);
            }
            let purpose = values[2].text().trim().to_string();
            let private = values[3].bool();
            let on_create = on_create.clone();
            cx.defer(move |cx| on_create(display_name, url_name, purpose, private, cx));
            true
        },
    );
}
