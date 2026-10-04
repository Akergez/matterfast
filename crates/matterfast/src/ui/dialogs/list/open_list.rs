use std::rc::Rc;

use gpui_kit::component::WindowExt;
use gpui_kit::prelude::*;
use gpui_kit::{px, App, Context, Entity, SharedString, Window};

use super::list_body::List;
use crate::ui::Ui;

/// Opens a list dialog around `build`'s sections and returns its body, which
/// is how rows get in afterwards.
pub(crate) fn open_list(
    ui: &Rc<Ui>,
    cx: &mut App,
    title: &str,
    build: impl FnOnce(&mut List, &mut Window, &mut Context<List>),
) -> Option<Entity<List>> {
    let title: SharedString = title.to_string().into();
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| {
            let mut list = List::new();
            build(&mut list, window, cx);
            list
        });
        let body = view.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title(title.clone()).w(px(520.)).child(body.clone())
        });
        view
    })
}
