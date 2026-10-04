use std::rc::Rc;

use gpui_kit::component::WindowExt;
use gpui_kit::{px, App, AppContext, ParentElement};

use super::dialog::Settings;
use crate::ui::Ui;

pub fn show(ui: &Rc<Ui>, cx: &mut App) {
    let session = ui.clone();
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| Settings::new(session, window, cx));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title("Settings").w(px(520.)).child(view.clone())
        });
    });
}
