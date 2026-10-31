use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::App;

use super::field::Field;
use super::form::Form;
use super::form_spec::FormSpec;
use super::value::Value;
use crate::ui::Ui;

/// Opens a form. `on_ok` gets the values and answers whether they were
/// acceptable: `false` keeps the dialog up so the person can fix them.
pub fn form(
    ui: &Rc<Ui>,
    cx: &mut App,
    spec: FormSpec,
    fields: Vec<Field>,
    on_ok: impl Fn(Vec<Value>, &mut App) -> bool + 'static,
) {
    let on_ok = Rc::new(on_ok);
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| {
            let mut form = Form::new(fields, window, cx);
            form.note = spec.note.clone();
            if let Some(derive) = spec.follow {
                form.follow(derive, window, cx);
            }
            form
        });
        let fields = view.clone();
        gpui_adaptive::Form::new(spec.title.clone(), view)
            .commit(spec.ok.clone(), move |_, cx| {
                let values = fields.read(cx).values(cx);
                // The answer has to be known synchronously, and the
                // callbacks here only read their arguments to decide it.
                on_ok(values, cx)
            })
            .open(window, cx);
    });
}
