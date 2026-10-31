use gpui_adaptive::prim::{row, spinner};
use gpui_adaptive::ActiveColors as _;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App};

use crate::ui::kit::{self, Lucide};
use crate::ui::{Action, Ui};

/// The files waiting to go out with the next message.
pub(super) fn attachments(ui: &Rc<Ui>, cx: &App) -> Option<AnyElement> {
    let files = ui.state.borrow().pending_files.clone();
    let uploading = ui.chat.uploading.get();
    if files.is_empty() && uploading == 0 {
        return None;
    }
    let chip = || {
        row()
            .gap_1()
            .px_2()
            .h(px(28.))
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(cx.colors().border)
            .bg(cx.colors().sunken.opacity(0.5))
            .text_sm()
    };
    let mut row = row().flex_none().flex_wrap().gap_1p5().px_3p5().pb_1();
    for (index, (id, name)) in files.into_iter().enumerate() {
        row = row.child(
            chip()
                .child(Lucide::Paperclip)
                .child(div().max_w(px(220.)).truncate().child(name))
                .child(
                    kit::icon_button(("remove", index), Lucide::X, "Remove")
                        .on_trigger(ui.run(move |ui, cx| {
                            ui.dispatch(Action::DropAttachment(id.clone()), cx)
                        }))
                        .as_icon_button().dense(),
                ),
        );
    }
    if uploading > 0 {
        row = row.child(chip().child(spinner()).child(match uploading {
            1 => "Uploading…".to_string(),
            n => format!("Uploading {n} files…"),
        }));
    }
    Some(row.into_any_element())
}
