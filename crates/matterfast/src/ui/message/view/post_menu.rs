use std::rc::Rc;

use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::{AnyElement, IntoElement};
use mattermost_api::models::Post;

use crate::ui::message::PostAction;
use crate::ui::{kit, Action, Ui};
use kit::Lucide;

/// What the "…" menu offers for a post. Editing and deleting are only offered
/// on your own posts — the server would refuse anyway, and an option that
/// always fails is worse than no option.
fn post_menu_entries(post: &Post, mine: bool) -> Vec<(&'static str, PostAction)> {
    let mut entries = vec![
        ("Copy text", PostAction::CopyText),
        ("Copy link", PostAction::CopyLink),
        ("Mark as unread", PostAction::MarkUnread),
        ("Forward…", PostAction::Forward),
        ("Remind me about this…", PostAction::Remind),
    ];
    if post.is_pinned {
        entries.push(("Unpin from channel", PostAction::Unpin));
    } else {
        entries.push(("Pin to channel", PostAction::Pin));
    }
    // Only worth offering where there is a history to see.
    if post.is_edited() {
        entries.push(("Edit history", PostAction::History));
    }
    if post.reply_count > 0 {
        entries.push(("Summarise thread", PostAction::Summarise));
        entries.push(("Move thread…", PostAction::MoveThread));
    }
    if mine {
        entries.push(("Edit", PostAction::Edit));
        entries.push(("Delete", PostAction::Delete));
    }
    entries
}

pub(super) fn overflow_menu(ui: &Rc<Ui>, post: &Post, mine: bool) -> AnyElement {
    let entries = post_menu_entries(post, mine);
    let post_id = post.id.clone();
    let ui = ui.clone();
    kit::icon_button("more-actions", Lucide::Ellipsis, "More actions")
        .dropdown_menu(move |mut menu, _, _| {
            for (label, action) in &entries {
                let action = *action;
                let post_id = post_id.clone();
                menu = menu.item(PopupMenuItem::new(*label).on_click(ui.click(
                    move |ui, cx| ui.dispatch(Action::Post(post_id.clone(), action), cx),
                )));
            }
            menu
        })
        .into_any_element()
}
