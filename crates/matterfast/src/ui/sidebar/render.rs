use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{canvas, div, list, px, AnyElement, App, FontWeight};

use super::channel_row::{channel_row, Fit};
use super::channel_sidebar::ROW_HEIGHT;
use super::folders::folders;
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

    let folders = folders(ui, &st, cx);
    // Putting the conversations in order is a sort of all of them, and is
    // not done for a frame that shows the inbox instead.
    let inbox = ui.channels.showing_inbox();
    let rows = match inbox {
        true => Rc::new(Vec::new()),
        false => ui
            .channels
            .rows(&st.chat_list(ui.channels.folder().as_deref()), theme.font_size),
    };
    drop(st);
    let last = rows.len().saturating_sub(1);
    let row_ui = ui.clone();
    let channels = list(ui.channels.list.clone(), move |index, _, cx| {
        let began = std::time::Instant::now();
        let st = row_ui.state.borrow();
        let row = rows
            .get(index)
            .and_then(|id| st.channels.get(id))
            .map(|channel| channel_row(&row_ui, channel, &st, Fit::LIST, cx));
        // A row the state no longer has is gone from the list one frame
        // later; until then it takes the room it was promised.
        let row = row.unwrap_or_else(|| div().h(ROW_HEIGHT).into_any_element());
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
        .pt_1()
        .child(channels);

    let pane = v_flex()
        .size_full()
        .bg(theme.sidebar)
        .text_color(theme.sidebar_foreground)
        .border_r_1()
        .border_color(theme.sidebar_border)
        .child(header);

    // The inbox takes the place of the conversations, under the same tabs.
    let body = match inbox {
        true => crate::ui::rhs::inbox_view(ui, cx),
        false => list.into_any_element(),
    };
    // Where the rows are is kept, for telling a swipe across them from one
    // across the tabs.
    let seen = ui.clone();
    let body = v_flex()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            canvas(move |bounds, _, _| seen.channels.body.set(bounds), |_, _, _, _| ())
                .absolute()
                .size_full(),
        )
        .child(body);
    pane.child(folders)
        .child(body)
        .when_some(dock, |pane, dock| pane.child(dock))
        .into_any_element()
}
