use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{canvas, div, list, px, AnyElement, App, ElementId, Window};

use super::channel_row::{channel_row, Fit};
use super::channel_sidebar::ROW_HEIGHT;
use super::folders::folders;
use super::main_menu::main_menu;
use super::switcher::switcher;
use crate::ui::kit::{self, Lucide};
use crate::ui::{MenuAction, Ui};

/// Draws the sidebar. `dock` is the call dock, which is pinned under the
/// channel list for as long as a call runs.
pub fn render(
    ui: &Rc<Ui>,
    dock: Option<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let st = ui.state.borrow();
    let theme = cx.theme().clone();
    let collapsed = ui.split.collapsed.get();
    let searching = collapsed && ui.search_box.compact_expanded();
    let header = h_flex()
        .id("sidebar-header")
        .flex_none()
        .h(px(48.))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(theme.sidebar_border)
        .child(switcher(ui, cx))
        .child(if searching {
            div()
                .flex_1()
                .min_w_0()
                .child(crate::ui::search::render(
                    ui,
                    (f32::from(window.viewport_size().width) - 128.0).max(100.0),
                    window,
                    cx,
                ))
                .into_any_element()
        } else {
            folders(ui, &st, cx)
        })
        .child(if searching {
            kit::icon_button("close-search", Lucide::X, "Close search")
                .on_click(ui.click(|ui, cx| {
                    ui.search_box.collapse_compact(cx);
                    ui.channels.refresh(cx);
                }))
        } else if collapsed {
            kit::icon_button("sidebar-search", Lucide::Search, "Search messages  (Ctrl+F)")
                .on_click(ui.click(|ui, cx| ui.menu_action(MenuAction::FocusSearch, cx)))
        } else {
            kit::icon_button("find-channel", Lucide::ListFilter, "Find channel  (Ctrl+K)")
                .on_click(ui.click(|ui, cx| ui.menu_action(MenuAction::QuickSwitch, cx)))
        })
        .child(main_menu(ui));

    let categories: Vec<String> = st
        .folders()
        .into_iter()
        .map(|folder| folder.id.clone())
        .collect();
    let open = ui
        .channels
        .folder()
        .and_then(|id| categories.iter().position(|folder| *folder == id));
    let now = match (ui.channels.showing_inbox(), open) {
        (true, _) => 0,
        (false, None) => 1,
        (false, Some(index)) => index + 2,
    };
    ui.channels.set_tab_order(now, categories.len() + 2);
    drop(st);

    let pane = v_flex()
        .size_full()
        // Rows on their way in from a side are not drawn past the column.
        .overflow_hidden()
        .bg(theme.sidebar)
        .text_color(theme.sidebar_foreground)
        .border_r_1()
        .border_color(theme.sidebar_border)
        .child(header);

    let slide = ui.channels.advance(window);
    let across = ui.channels.body.get().size.width;
    let seen = ui.clone();
    let current = page(ui, now, &categories, cx);
    let neighbor = if slide < 0.0 {
        now.checked_add(1)
    } else {
        now.checked_sub(1)
    }
    .filter(|next| *next < categories.len() + 2 && slide != 0.0);
    let body = div()
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_hidden()
        .child(
            canvas(
                move |bounds, _, _| seen.channels.body.set(bounds),
                |_, _, _, _| (),
            )
            .absolute()
            .size_full(),
        )
        .child(
            v_flex()
                .id(page_id(now, &categories))
                .absolute()
                .top_0()
                .bottom_0()
                .left(across * slide)
                .w_full()
                .child(current),
        )
        .when_some(neighbor, |body, next| {
            let offset = slide + if slide < 0.0 { 1.0 } else { -1.0 };
            body.child(
                v_flex()
                    .id(page_id(next, &categories))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(across * offset)
                    .w_full()
                    .child(page(ui, next, &categories, cx)),
            )
        });
    pane.child(body)
        .when_some(dock, |pane, dock| pane.child(dock))
        .into_any_element()
}

fn page(ui: &Rc<Ui>, index: usize, categories: &[String], cx: &mut App) -> AnyElement {
    if index == 0 {
        return crate::ui::rhs::inbox_view(ui, cx);
    }
    let folder = index
        .checked_sub(2)
        .and_then(|index| categories.get(index))
        .cloned();
    let st = ui.state.borrow();
    let (state, rows) = ui.channels.rows(
        folder.clone(),
        &st.chat_list(folder.as_deref()),
        cx.theme().font_size,
    );
    drop(st);
    let last = rows.len().saturating_sub(1);
    let row_ui = ui.clone();
    let channels = list(state, move |index, _, cx| {
        let began = std::time::Instant::now();
        let st = row_ui.state.borrow();
        let row = rows
            .get(index)
            .and_then(|id| st.channels.get(id))
            .map(|channel| channel_row(&row_ui, channel, &st, Fit::LIST, cx))
            .unwrap_or_else(|| div().h(ROW_HEIGHT).into_any_element());
        crate::ui::frame_log::row(crate::ui::Part::Sidebar, began);
        div()
            .w_full()
            .when(index == last, |row| row.pb_2())
            .child(row)
            .into_any_element()
    })
    .size_full();
    div()
        .id("channels")
        .size_full()
        .min_h_0()
        .px_1p5()
        .pt_1()
        .child(channels)
        .into_any_element()
}

fn page_id(index: usize, categories: &[String]) -> ElementId {
    let key = match index {
        0 => "inbox",
        1 => "all",
        index => categories
            .get(index - 2)
            .map(String::as_str)
            .unwrap_or("all"),
    };
    ElementId::Name(format!("category-page-{key}").into())
}
