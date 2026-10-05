use std::rc::Rc;

use gpui_kit::component::h_flex;
use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, AnyElement, App, ObjectFit};

use crate::ui::{kit, Ui};

/// Remote screens and cameras, floating over the panes rather than in them:
/// a share is something you glance at while carrying on reading.
pub(super) fn videos(ui: &Rc<Ui>, cx: &App) -> Option<AnyElement> {
    let views = ui.video_views.borrow();
    if views.is_empty() {
        return None;
    }
    let mut row = h_flex()
        .absolute()
        .left_3()
        .bottom_3()
        .gap_2()
        .items_end();
    for (index, (_, view)) in views.iter().enumerate() {
        let (width, height) = view.size();
        let expanded = view.expanded.get();
        let frame = div()
            .id(("video", index))
            .w(px(width))
            .h(px(height))
            .max_w_full()
            .rounded_md()
            .overflow_hidden()
            .bg(gpui_kit::black())
            .shadow_lg()
            .cursor_pointer()
            .when_some(view.image(), |frame, picture| {
                frame.child(img(picture).size_full().object_fit(ObjectFit::Contain))
            })
            .on_click(ui.click(move |ui, cx| {
                if let Some((_, view)) = ui.video_views.borrow().get(index) {
                    view.expanded.set(!view.expanded.get());
                }
                crate::ui::redraw(&[crate::ui::Part::Frame], cx);
            }));
        row = row.child(kit::with_tooltip(
            ("video-tip", index),
            frame,
            format!(
                "{} — click to {}",
                view.title,
                if expanded { "shrink" } else { "expand" }
            ),
        ));
    }
    let _ = cx;
    Some(row.into_any_element())
}
