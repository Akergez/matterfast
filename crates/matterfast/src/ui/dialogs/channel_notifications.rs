use std::rc::Rc;

use gpui_kit::App;

use super::forms::{form, Field, FormSpec};
use super::levels::CHANNEL_LEVELS;
use crate::ui::Ui;

/// Per-channel notification settings. `current` is (desktop_level,
/// mark_unread_all, ignore_channel_mentions) where desktop_level is one of
/// "default"|"all"|"mention"|"none". Answers the same triple.
pub fn channel_notifications(
    ui: &Rc<Ui>,
    cx: &mut App,
    channel_name: &str,
    current: (String, bool, bool),
    on_save: impl Fn(String, bool, bool, &mut App) + 'static,
) {
    let (level, mark_unread_all, ignore_channel_mentions) = current;
    let on_save = Rc::new(on_save);
    form(
        ui,
        cx,
        FormSpec::new(&format!("Notifications for {channel_name}"), "Save"),
        vec![
            Field::choice("Desktop notifications", &CHANNEL_LEVELS, &level),
            Field::switch(
                "Mark as unread",
                "For every message, not only mentions",
                mark_unread_all,
            ),
            Field::switch(
                "Ignore @channel, @here and @all",
                "",
                ignore_channel_mentions,
            ),
        ],
        move |values, cx| {
            let (level, unread, ignore) = (values[0].text(), values[1].bool(), values[2].bool());
            let on_save = on_save.clone();
            cx.defer(move |cx| on_save(level, unread, ignore, cx));
            true
        },
    );
}
