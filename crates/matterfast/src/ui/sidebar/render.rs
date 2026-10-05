use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, list, px, AnyElement, App, FontWeight};

use super::category_header::category_header;
use super::channel_row::channel_row;
use super::channel_sidebar::{Row, ROW_HEIGHT};
use super::main_menu::main_menu;
use super::switcher::switcher;
use crate::ui::kit::{self, Lucide};
use crate::ui::{MenuAction, Ui};

/// Draws the sidebar. `dock` is the call dock, which is pinned under the
/// channel list for as long as a call runs.
pub fn render(ui: &Rc<Ui>, dock: Option<AnyElement>, cx: &mut App) -> AnyElement {
    let st = ui.state.borrow();
    let theme = cx.theme();
    let team_name = st
        .current_team
        .as_ref()
        .and_then(|id| st.teams.iter().find(|t| &t.id == id))
        .map(|t| t.display_name.clone())
        .unwrap_or_else(|| "Matterfast".to_string());

    let header = h_flex()
        .flex_none()
        .h(px(48.))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(theme.sidebar_border)
        .child(switcher(ui, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(team_name),
        )
        .child(
            // Going to a channel by name: the list's own search, as opposed
            // to the search of messages in the title bar.
            kit::icon_button("find-channel", Lucide::ListFilter, "Find channel  (Ctrl+K)")
                .on_click(ui.click(|ui, cx| ui.menu_action(MenuAction::QuickSwitch, cx))),
        )
        .child(main_menu(ui));

    let rows = ui.channels.rows(&st.sidebar_groups());
    drop(st);
    let last = rows.len().saturating_sub(1);
    let row_ui = ui.clone();
    let channels = list(ui.channels.list.clone(), move |index, _, cx| {
        let began = std::time::Instant::now();
        let st = row_ui.state.borrow();
        let row = match rows.get(index) {
            Some(Row::Category(id)) => st
                .categories
                .categories
                .iter()
                .find(|category| &category.id == id)
                .map(|category| category_header(&row_ui, category, cx)),
            Some(Row::Channel(id)) => st
                .channels
                .get(id)
                .map(|channel| channel_row(&row_ui, channel, &st, cx)),
            None => None,
        };
        // A row the state no longer has is gone from the list one frame
        // later; until then it takes the room it was promised.
        let row = row.unwrap_or_else(|| div().h(px(ROW_HEIGHT)).into_any_element());
        crate::ui::frame_log::row(crate::ui::Part::Sidebar, began);
        div()
            .w_full()
            .when(index == last, |row| row.pb_2())
            .child(row)
            .into_any_element()
    })
    .size_full();
    let list = div()
        .id("channels")
        .flex_1()
        .min_h_0()
        .px_1p5()
        .child(channels);

    let pane = v_flex()
        .size_full()
        .bg(theme.sidebar)
        .text_color(theme.sidebar_foreground)
        .border_r_1()
        .border_color(theme.sidebar_border)
        .child(header);

    pane.child(list)
        .when_some(dock, |pane, dock| pane.child(dock))
        .into_any_element()
}
