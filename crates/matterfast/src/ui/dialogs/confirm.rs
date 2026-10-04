use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, App, SharedString};

use super::callback::{run_later, Callback};
use crate::ui::Ui;

/// Asks a yes-or-no question. `danger` paints the confirming button as
/// something that cannot be taken back.
pub fn confirm(
    ui: &Rc<Ui>,
    cx: &mut App,
    title: &str,
    body: &str,
    ok: &str,
    danger: bool,
    on_ok: impl Fn(&mut App) + 'static,
) {
    let title: SharedString = title.to_string().into();
    let body: SharedString = body.to_string().into();
    let ok: SharedString = ok.to_string().into();
    let on_ok: Callback = Rc::new(on_ok);
    ui.with_window(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, _, cx| {
            let on_ok = on_ok.clone();
            let confirm = Button::new("ok").label(ok.clone());
            dialog
                .title(title.clone())
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(body.clone()),
                )
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().child(Button::new("cancel").label("Cancel")))
                        .child(DialogAction::new().child(if danger {
                            confirm.danger()
                        } else {
                            confirm.primary()
                        })),
                )
                .on_ok(move |_, _, cx| {
                    run_later(&on_ok, cx);
                    true
                })
        });
    });
}
