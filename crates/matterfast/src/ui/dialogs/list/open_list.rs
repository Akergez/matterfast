use std::rc::Rc;

use gpui_adaptive::{Sheet, SheetOptions};
use gpui_kit::prelude::*;
use gpui_kit::{rems, App, Context, Entity, SharedString, Window};

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
        let options = SheetOptions { title: Some(title), width: Some(rems(32.5)), ..Default::default() };
        Sheet::open_with(view.clone(), options, window, cx);
        view
    })
}
