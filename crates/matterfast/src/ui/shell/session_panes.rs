use std::rc::Rc;

use gpui_adaptive::{Env, Navigator, Sheet, WidthClass};
use gpui_kit::prelude::*;
use gpui_kit::{canvas, div, px, AnyElement, App, DispatchPhase, ScrollWheelEvent, Window};

use super::column_width::{column_width, dragged_width};
use super::constants::{DRAGGED_MIN, DRAGGED_SHARE};
use super::panes::Panes;
use super::videos::videos;
use crate::ui::layout::Place;
use crate::ui::rhs::PanelMode;
use crate::ui::{Divider, Ui};

/// The session's places, laid out by the design system's navigator for
/// however wide the window is right now: side by side where there is room,
/// a stack of pages on a phone (see [`Place`]).
pub(super) fn session(ui: &Rc<Ui>, columns: &Panes, window: &mut Window, cx: &mut App) -> AnyElement {
    ui.learn_unknown_mentions();
    ui.scale.set(window.scale_factor());
    let collapsed = Env::of(window, cx).width() == WidthClass::Compact;
    ui.pages.collapsed.set(collapsed);
    ui.widths.save_when_settled(cx);
    panel_closed(ui, cx);

    let navigator = ui.pages.attach(cx);
    let pages = {
        let columns = columns.clone();
        Navigator::new("places", &navigator, move |place, _, _, _| match place {
            Place::List => columns.sidebar(),
            Place::Chat => columns.chat(),
            Place::Thread | Place::Search => columns.right(),
        })
    };
    // The side columns are as wide as the person dragged them, within
    // reason: not thinner than a channel name, not so wide the
    // conversation has no room.
    let resizable = {
        let (sized, resized) = (ui.clone(), ui.clone());
        pages.resizable(
            move |place, window_width, _| px(side_width(&sized, *place, f32::from(window_width))),
            move |place, width, cx| {
                let divider = match place {
                    Place::List => Divider::Sidebar,
                    _ => Divider::Panel,
                };
                resized.widths.set(divider, width.map(f32::from), cx);
            },
        )
    };

    let mut panes = div().relative().size_full().min_h_0().child(resizable);

    // On a phone a swipe sideways over the list goes to the tab beside the
    // one in front; over a page above it the navigator takes the swipe to
    // the right as back. A script may swipe on any system: it is how this
    // is tested where there is no finger.
    let swipes = cfg!(target_os = "android") || crate::ui::script::playing();
    let list_in_front = navigator.read(cx).stack() == [Place::List];
    if collapsed && swipes && list_in_front && !Sheet::is_open(window, cx) {
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
                        let (taken, turn) = ui.channels.swiped(dx, dy, event.touch_phase, event.position);
                        if let Some(step) = turn {
                            crate::ui::sidebar::turn(&ui, step, cx);
                        }
                        if taken {
                            window.refresh();
                            cx.stop_propagation();
                        }
                    });
                },
            )
            .absolute()
            .size_0(),
        );
    }

    panes
        .when_some(videos(ui, cx), |panes, videos| panes.child(videos))
        .when_some(crate::ui::lightbox::render(ui, cx), |panes, lightbox| panes.child(lightbox))
        .into_any_element()
}

/// How wide a side column is in a window `width` wide: what it was dragged
/// to, or its share of the window.
fn side_width(ui: &Ui, place: Place, width: f32) -> f32 {
    let most = (width * DRAGGED_SHARE).max(DRAGGED_MIN);
    match place {
        Place::List => dragged_width(ui.widths.get(Divider::Sidebar), column_width(width, 0.24, 220.0, 360.0), DRAGGED_MIN, most),
        _ => dragged_width(ui.widths.get(Divider::Panel), column_width(width, 0.30, 320.0, 460.0), DRAGGED_MIN, most),
    }
}

/// Keeps what the panel shows in step with the pages: a panel taken away
/// by back, a swipe or Escape shows nothing any more.
fn panel_closed(ui: &Rc<Ui>, cx: &mut App) {
    if ui.pages.panel(cx).is_none() && !matches!(ui.right.mode(cx), PanelMode::Hidden) {
        ui.later(cx, |ui, cx| {
            if ui.pages.panel(cx).is_none() {
                ui.right.set_mode(PanelMode::Hidden, cx);
            }
        });
    }
}
