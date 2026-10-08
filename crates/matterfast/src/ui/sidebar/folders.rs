use std::rc::Rc;

use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{px, AnyElement, App, ElementId, FontWeight, SharedString};
use mattermost_api::models::CategoryType;

use super::row_action::RowAction;
use super::tab_swipe::turned;
use crate::state::AppState;
use crate::ui::{kit, Action, Ui};

/// What a folder's tab has to show besides its name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Waiting {
    /// Mentions in the folder's conversations, muted ones included: being
    /// named is what muting does not silence.
    pub(super) mentions: i64,
    /// Whether anything else in it is unread and not muted.
    pub(super) unread: bool,
}

/// What is waiting in a set of conversations. Asked for every tab on every
/// frame the tabs are drawn in — a frame of scrolling the list under them
/// is one — so it is a walk and no more: nothing is sorted to be counted.
pub(super) fn waiting<'a>(
    st: &AppState,
    channels: impl Iterator<Item = &'a mattermost_api::models::Channel>,
) -> Waiting {
    let mut waiting = Waiting::default();
    for channel in channels {
        let unread = st.unread(&channel.id);
        waiting.mentions += unread.mentions;
        waiting.unread |= unread.is_unread() && !unread.muted;
    }
    waiting
}

/// Goes to the tab `step` places along from the one in front, in the order
/// they are drawn in: the inbox, every conversation, the folders. What a
/// swipe across the list does on a phone.
pub(crate) fn turn(ui: &Rc<Ui>, step: i32, cx: &mut App) {
    let folders: Vec<String> = {
        let st = ui.state.borrow();
        st.folders().into_iter().map(|folder| folder.id.clone()).collect()
    };
    let open = ui
        .channels
        .folder()
        .and_then(|id| folders.iter().position(|folder| *folder == id));
    let now = match (ui.channels.showing_inbox(), open) {
        (true, _) => 0,
        (false, None) => 1,
        (false, Some(folder)) => folder + 2,
    };
    let next = turned(now, step, folders.len() + 2);
    if next == now {
        return;
    }
    match next {
        0 => ui.dispatch(Action::OpenInbox, cx),
        1 => ui.channels.set_folder(None, cx),
        folder => ui.channels.set_folder(folders.get(folder - 2).cloned(), cx),
    }
    // The next tab comes in from the right, the one before from the left.
    ui.channels.slide_from(if next > now { 1.0 } else { -1.0 }, cx);
}

/// The tabs over the list: the inbox first, every conversation second, then
/// the server's sidebar categories in its order as folders. A row that is
/// wider than the column scrolls sideways, as the same row does in Telegram.
///
/// The inbox is not a folder — it holds messages, not conversations — but it
/// is what the column shows instead of them, and so it is chosen where they
/// are.
pub(super) fn folders(ui: &Rc<Ui>, st: &AppState, cx: &App) -> AnyElement {
    let categories = st.folders();
    let theme = cx.theme();
    let inbox = ui.channels.showing_inbox();
    let open = ui.channels.folder();
    // A folder that is gone shows everything, and so "all" is what is open.
    let open = open.filter(|id| categories.iter().any(|category| category.id == *id));

    let tab = |id: ElementId, name: SharedString, selected: bool, waiting: Waiting| {
        h_flex()
            .id(id)
            .flex_none()
            .h(px(28.))
            .px_2p5()
            .gap_1p5()
            .items_center()
            .rounded_full()
            .cursor_pointer()
            .text_sm()
            .when(selected, |tab| {
                tab.bg(theme.sidebar_accent).font_weight(FontWeight::SEMIBOLD)
            })
            .when(!selected, |tab| {
                tab.text_color(theme.muted_foreground)
                    .hover(|style| style.bg(theme.list_hover))
            })
            .child(name)
            .when(waiting.mentions > 0, |tab| {
                tab.child(kit::mention_badge(waiting.mentions, false, cx))
            })
            .when(waiting.mentions == 0 && waiting.unread, |tab| {
                tab.child(kit::unread_dot(cx))
            })
    };
    let folder = |id: ElementId, name: SharedString, folder: Option<String>, waiting: Waiting| {
        let selected = !inbox && open == folder;
        tab(id, name, selected, waiting)
            .on_click(ui.click(move |ui, cx| ui.channels.set_folder(folder.clone(), cx)))
    };

    let mut row = h_flex()
        .id("folders")
        .flex_none()
        .w_full()
        .px_2()
        .py_1p5()
        .gap_1()
        .overflow_x_scroll()
        .border_b_1()
        .border_color(theme.sidebar_border)
        .child(
            // What is waiting in it is a count of its own: mentions and
            // threads, not conversations. Pressed while it is in front it
            // stays there: a tab is not a switch.
            tab(
                "folder-inbox".into(),
                "Inbox".into(),
                inbox,
                Waiting { mentions: st.inbox_waiting(), unread: false },
            )
            .on_click(ui.click(|ui, cx| {
                if !ui.channels.showing_inbox() {
                    ui.dispatch(Action::OpenInbox, cx);
                }
            })),
        )
        .child(folder("folder-all".into(), "All".into(), None, waiting(st, st.chats())));

    for category in categories {
        let id = ElementId::Name(format!("folder-{}", category.id).into());
        let tab = folder(
            id,
            category.display_name.clone().into(),
            Some(category.id.clone()),
            waiting(st, st.channels_of(category)),
        );
        // Custom categories can be renamed and deleted; the built-in ones
        // (Favourites, Channels, Direct Messages) cannot, and offering it
        // would only produce a server error.
        if category.r#type != CategoryType::Custom {
            row = row.child(tab);
            continue;
        }
        let ui = ui.clone();
        let category_id = category.id.clone();
        row = row.child(tab.context_menu(move |menu, _, _| {
            let rename = category_id.clone();
            let delete = category_id.clone();
            menu.item(PopupMenuItem::new("Rename…").on_click(ui.click(move |ui, cx| {
                ui.dispatch(Action::Row(rename.clone(), RowAction::RenameCategory), cx)
            })))
            .item(PopupMenuItem::new("Delete").on_click(ui.click(move |ui, cx| {
                ui.dispatch(Action::Row(delete.clone(), RowAction::DeleteCategory), cx)
            })))
        }));
    }
    row.into_any_element()
}
