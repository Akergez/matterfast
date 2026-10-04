use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::WindowExt;
use gpui_kit::prelude::*;
use gpui_kit::{App, SharedString};

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
        let title: SharedString = spec.title.clone().into();
        let ok: SharedString = spec.ok.clone().into();
        window.open_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let submit = view.clone();
            let on_ok = on_ok.clone();
            dialog
                .title(title.clone())
                .child(view)
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().child(Button::new("cancel").label("Cancel")))
                        .child(
                            DialogAction::new()
                                .child(Button::new("ok").label(ok.clone()).primary()),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    let values = submit.read(cx).values(cx);
                    // The answer has to be known synchronously, and the
                    // callbacks here only read their arguments to decide it.
                    on_ok(values, cx)
                })
        });
    });
}
