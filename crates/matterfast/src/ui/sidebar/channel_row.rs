use std::rc::Rc;

use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, ElementId, FontWeight};
use mattermost_api::models::{Channel, ChannelType};

use super::call_badge::call_badge;
use super::row_menu::row_menu;
use crate::state::AppState;
use crate::ui::kit::{self, Lucide};
use crate::ui::message::{custom_status_tooltip, emoji_element, status_is_live};
use crate::ui::{Action, Ui};

pub(super) fn channel_row(ui: &Rc<Ui>, channel: &Channel, st: &AppState, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let title = st.channel_title(channel);
    let unread = st.unread(&channel.id);
    let selected = st.current_channel.as_deref() == Some(channel.id.as_str());
    let teammate = channel.dm_teammate_id(&st.me.id);

    // Public channels get a literal "#", the way Mattermost writes them; a
    // DM is a person, so it gets that person's face with the presence dot,
    // exactly like a message row.
    let icon: AnyElement = match (teammate, &channel.r#type) {
        (Some(user_id), _) => {
            kit::avatar_with_presence(ui, user_id, &title, 20., st.presence(user_id), cx)
        }
        (None, ChannelType::Open) => div()
            .w(px(20.))
            .flex_none()
            .text_center()
            .text_color(theme.muted_foreground)
            .child("#")
            .into_any_element(),
        (None, kind) => div()
            .w(px(20.))
            .flex_none()
            .flex()
            .justify_center()
            .text_color(theme.muted_foreground)
            .child(match kind {
                ChannelType::Private => Lucide::Lock,
                ChannelType::Group => Lucide::Users,
                _ => Lucide::User,
            })
            .into_any_element(),
    };

    let mut row = h_flex()
        .id(ElementId::Name(format!("channel-{}", channel.id).into()))
        .w_full()
        .h(px(32.))
        .px_2()
        .gap_2()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.sidebar_accent))
        .when(!selected, |row| row.hover(|style| style.bg(theme.list_hover)))
        // Muted channels still show mentions, but not bold-for-messages: that
        // is exactly the rule the official clients use.
        .when(unread.muted, |row| row.opacity(0.6))
        .child(icon)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(unread.is_unread(), |label| {
                    label.font_weight(FontWeight::SEMIBOLD)
                })
                .child(title),
        );

    // A DM is a person, and their status says whether writing to them is
    // worth doing now.
    if let Some(status) = teammate
        .and_then(|id| st.users.get(id))
        .and_then(|user| user.custom_status())
        .filter(|status| !status.emoji.is_empty() && status_is_live(status))
    {
        row = row.child(kit::with_tooltip(
            "status",
            emoji_element(ui, &status.emoji, 16.),
            custom_status_tooltip(&status),
        ));
    }

    // A call in this channel matters more than an unread badge, so it goes
    // first and is always shown.
    if let Some(people) = st.active_calls.get(&channel.id) {
        row = row.child(call_badge(ui, people, st, cx));
    }

    // Something unsent here. Shown even on a muted channel: it is your own
    // text waiting, not someone else's noise.
    if st.drafts.contains_key(&channel.id) {
        row = row.child(kit::with_tooltip(
            "draft",
            div()
                .text_color(theme.muted_foreground)
                .child(Lucide::Pencil),
            "You have an unsent message here",
        ));
    }

    if unread.mentions > 0 {
        row = row.child(kit::mention_badge(unread.mentions, unread.urgent, cx));
    } else if unread.is_unread() && !unread.muted {
        row = row.child(kit::unread_dot(cx));
    }

    let entries = row_menu(&channel.id, st);
    let menu_ui = ui.clone();
    let channel_id = channel.id.clone();
    let select = channel.id.clone();
    row.on_click(ui.click(move |ui, cx| {
        ui.dispatch(Action::SelectChannel(select.clone()), cx)
    }))
    // Mute it, mark it, or file it under a different category: all things
    // people expect to reach from the row itself rather than from a settings
    // screen.
    .context_menu(move |mut menu, _, _| {
        for (label, action) in &entries {
            let action = action.clone();
            let channel_id = channel_id.clone();
            menu = menu.item(PopupMenuItem::new(label.clone()).on_click(menu_ui.click(
                move |ui, cx| ui.dispatch(Action::Row(channel_id.clone(), action.clone()), cx),
            )));
        }
        menu
    })
    .into_any_element()
}
