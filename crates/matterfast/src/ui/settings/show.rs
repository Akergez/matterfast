use std::rc::Rc;

use gpui_adaptive::{Sheet, SheetOptions};
use gpui_kit::{rems, App, AppContext};

use super::dialog::Settings;
use crate::ui::Ui;

pub fn show(ui: &Rc<Ui>, cx: &mut App) {
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| Settings::new(window, cx));
        let options = SheetOptions { title: Some("Settings".into()), width: Some(rems(32.5)), ..Default::default() };
        Sheet::open_with(view, options, window, cx);
    });
}
