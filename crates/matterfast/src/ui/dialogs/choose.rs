use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::{v_flex, ActiveTheme, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{div, App, SharedString};

use super::callback::{run_later, Callback};
use crate::ui::Ui;

/// Offers a handful of answers as buttons, one of which is picked.
pub(crate) fn choose(
    ui: &Rc<Ui>,
    cx: &mut App,
    title: &str,
    body: &str,
    options: Vec<(&'static str, Callback)>,
) {
    let title: SharedString = title.to_string().into();
    let body: SharedString = body.to_string().into();
    let options = Rc::new(options);
    ui.with_window(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, _, cx| {
            let mut column = v_flex().gap_2().child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(body.clone()),
            );
            for (index, (label, action)) in options.iter().enumerate() {
                let action = action.clone();
                column = column.child(
                    Button::new(("option", index))
                        .label(*label)
                        .w_full()
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            run_later(&action, cx);
                        }),
                );
            }
            dialog.title(title.clone()).child(column)
        });
    });
}
