use std::rc::Rc;

use gpui_kit::App;

use super::open_list::open_list;
use super::row::Row;
use super::section::Empty;
use crate::ui::Ui;

/// Shows rows that are already known: an edit history, what is scheduled.
pub fn show_rows(ui: &Rc<Ui>, cx: &mut App, title: &str, rows: Vec<Row>, empty: Option<Empty>) {
    open_list(ui, cx, title, move |list, _, cx| {
        let section = list.section("", empty);
        list.set_rows(section, rows, cx);
    });
}
