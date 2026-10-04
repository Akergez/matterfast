use std::rc::Rc;

use gpui_kit::component::{h_flex, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::{
    canvas, div, px, AnyElement, App, DispatchPhase, DragMoveEvent, ScrollWheelEvent, Window,
};

use super::column_width::{column_width, dragged_width};
use super::constants::{
    DRAGGED_MIN, DRAGGED_SHARE, SIDEBAR_PAGE_BELOW, STATIC_PANEL_MIN_WIDTH,
};
use super::divider::divider;
use super::videos::videos;
use crate::ui::rhs::PanelMode;
use crate::ui::{Divider, Ui};

/// The three panes, laid out for however wide the window is right now.
pub(super) fn session(ui: &Rc<Ui>, window: &mut Window, cx: &mut App) -> AnyElement {
    ui.learn_unknown_mentions();
    let width = f32::from(window.viewport_size().width);
    ui.scale.set(window.scale_factor());
    let collapsed = width < SIDEBAR_PAGE_BELOW;
    let narrow = width < STATIC_PANEL_MIN_WIDTH;
    ui.split.collapsed.set(collapsed);
    ui.narrow.set(narrow);

    // A thread is a place you read alongside the conversation, so it earns a
    // static column when there is room. The inbox is a stack you glance at
    // and dismiss, so it always overlays — pushing the conversation aside for
    // it would be a heavier gesture than the content deserves.
    let overlays = narrow
        || matches!(
            ui.right.mode(cx),
            PanelMode::Inbox | PanelMode::Search(_)
        );
    let panel = ui
        .overlay
        .shown()
        .then(|| crate::ui::rhs::render(ui, cx))
        .flatten();
    let dock = crate::ui::call_dock::render(ui, cx);

    // On a phone the panel is a page: a strip of conversation left showing
    // beside it is too narrow to read and too easy to tap by accident.
    ui.widths.save_when_settled(cx);
    let most = (width * DRAGGED_SHARE).max(DRAGGED_MIN);
    let sidebar_width = dragged_width(
        ui.widths.get(Divider::Sidebar),
        column_width(width, 0.24, 220.0, 360.0),
        DRAGGED_MIN,
        most,
    );
    let panel_width = if collapsed {
        width
    } else {
        dragged_width(
            ui.widths.get(Divider::Panel),
            column_width(width, 0.30, 320.0, 460.0),
            DRAGGED_MIN,
            most,
        )
    };
    let mut panes = h_flex().relative().size_full().min_h_0().on_drag_move({
        let ui = ui.clone();
        move |moved: &DragMoveEvent<Divider>, _, cx| {
            let which = *moved.drag(cx);
            let pointer = moved.event.position.x;
            let dragged = match which {
                Divider::Sidebar => pointer - moved.bounds.left(),
                Divider::Panel => moved.bounds.right() - pointer,
            };
            ui.widths.set(which, Some(f32::from(dragged).round()), cx);
        }
    });

    if collapsed {
        // The conversation, and the channel list as a drawer that slides in
        // over it from the left. The dock would come and go with the drawer,
        // so here it belongs under the conversation.
        let drawer = (width * 0.86).min(360.0);
        let open = ui.split.advance(window);
        panes = panes.child(
            div()
                .size_full()
                .child(crate::ui::chat::render(ui, true, dock, cx)),
        );
        if open > 0.0 {
            panes = panes
                .child(
                    div()
                        .id("drawer-scrim")
                        .absolute()
                        .inset_0()
                        .bg(gpui_kit::black().opacity(0.45 * open))
                        .occlude()
                        .on_click(ui.click(|ui, cx| ui.split.set_show_content(true, cx))),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px((open - 1.0) * drawer))
                        .w(px(drawer))
                        .shadow_lg()
                        .occlude()
                        .child(crate::ui::sidebar::render(ui, None, cx)),
                );
        }
        // A phone has no edge to click: the list is pulled out by a swipe
        // to the right and pushed back by one to the left. Anything laid
        // over the conversation keeps its own swipes.
        if cfg!(target_os = "android") && !ui.overlay.shown() && !window.has_active_dialog(cx) {
            let ui = ui.clone();
            panes = panes.child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let delta = event.delta.pixel_delta(px(1.));
                            let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
                            if ui.split.swiped(dx, dy, event.touch_phase, drawer) {
                                cx.stop_propagation();
                                window.refresh();
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
    } else {
        panes = panes
            .child(
                div()
                    .flex_none()
                    .h_full()
                    .w(px(sidebar_width))
                    .child(crate::ui::sidebar::render(ui, dock, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(crate::ui::chat::render(ui, false, None, cx)),
            );
    }

    if let Some(panel) = panel {
        panes = panes.child(if overlays || collapsed {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(panel_width))
                .shadow_lg()
                .occlude()
                .child(panel)
                .into_any_element()
        } else {
            div()
                .flex_none()
                .h_full()
                .w(px(panel_width))
                .child(panel)
                .into_any_element()
        });
        if !collapsed {
            panes = panes.child(divider(ui, Divider::Panel, panel_width, cx));
        }
    }
    // After the panes, so that it is above both of the two it sits between.
    if !collapsed {
        panes = panes.child(divider(ui, Divider::Sidebar, sidebar_width, cx));
    }

    panes
        .when_some(videos(ui, cx), |panes, videos| panes.child(videos))
        .when_some(crate::ui::lightbox::render(ui, cx), |panes, lightbox| {
            panes.child(lightbox)
        })
        .into_any_element()
}
