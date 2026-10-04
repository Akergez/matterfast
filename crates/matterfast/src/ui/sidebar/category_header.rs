use std::rc::Rc;

use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, App, ElementId, FontWeight};
use mattermost_api::models::{CategoryType, SidebarCategory};

use super::row_action::RowAction;
use crate::ui::{Action, Ui};

pub(super) fn category_header(ui: &Rc<Ui>, category: &SidebarCategory, cx: &App) -> AnyElement {
    let header = div()
        .id(ElementId::Name(format!("category-{}", category.id).into()))
        .w_full()
        .px_3()
        .pt_3()
        .pb_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(category.display_name.clone());

    // Custom categories can be renamed and deleted; the built-in ones
    // (Favourites, Channels, Direct Messages) cannot, and offering it would
    // only produce a server error.
    if category.r#type != CategoryType::Custom {
        return header.into_any_element();
    }
    let ui = ui.clone();
    let id = category.id.clone();
    header
        .context_menu(move |menu, _, _| {
            let rename = id.clone();
            let delete = id.clone();
            menu.item(PopupMenuItem::new("Rename…").on_click(ui.click(move |ui, cx| {
                ui.dispatch(Action::Row(rename.clone(), RowAction::RenameCategory), cx)
            })))
            .item(PopupMenuItem::new("Delete").on_click(ui.click(move |ui, cx| {
                ui.dispatch(Action::Row(delete.clone(), RowAction::DeleteCategory), cx)
            })))
        })
        .into_any_element()
}
