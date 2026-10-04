//! Sidebar categories and the menu on a channel row.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::{CategoryType, SidebarCategory, StringMap};

use super::ui::Ui;
use crate::runtime;
use crate::ui::dialogs;
use crate::ui::sidebar::RowAction;

impl Ui {
    /// Muting a channel, or filing it under a different category.
    pub(crate) fn row_action(self: &Rc<Self>, channel_id: String, what: RowAction, cx: &mut App) {
        let (client, me, team_id) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.me.id.clone(),
                st.current_team.clone().unwrap_or_default(),
            )
        };

        match what {
            RowAction::MarkRead => {
                let crt = self.state.borrow().crt_enabled;
                let ui = self.clone();
                runtime::spawn(
                    async move { client.view_channel(&channel_id, "", crt).await },
                    move |result, cx| match result {
                        // The server broadcasts multiple_channels_viewed, which
                        // is what actually clears the badge.
                        Ok(_) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not mark it read: {e}"), cx),
                    },
                );
            }
            RowAction::MarkUnread => {
                let crt = self.state.borrow().crt_enabled;
                let ui = self.clone();
                let target = channel_id.clone();
                runtime::spawn(
                    async move { client.mark_channel_unread(&me, &channel_id, crt).await },
                    move |result, cx| match result {
                        Ok(()) => {
                            // Reading it again on the way out would undo this.
                            if ui.state.borrow().current_channel.as_deref() == Some(target.as_str())
                            {
                                ui.state.borrow_mut().current_channel = None;
                                ui.refresh_messages(cx);
                            }
                            ui.schedule_sidebar_reload(cx);
                        }
                        Err(e) => ui.toast(&format!("Could not mark it unread: {e}"), cx),
                    },
                );
            }
            RowAction::SetMuted(muted) => {
                // Muted is stored as mark_unread: "mention" — the same field
                // the notification dialog writes, so it goes the same way.
                let mut props = StringMap::new();
                props.insert(
                    "mark_unread".into(),
                    if muted { "mention" } else { "all" }.into(),
                );
                let ui = self.clone();
                runtime::spawn(
                    async move {
                        client
                            .set_channel_notify_props(&channel_id, &me, &props)
                            .await
                    },
                    move |result, cx| match result {
                        Ok(()) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not change that: {e}"), cx),
                    },
                );
            }
            RowAction::RenameCategory => {
                let current = self
                    .state
                    .borrow()
                    .categories
                    .categories
                    .iter()
                    .find(|c| c.id == channel_id)
                    .map(|c| c.display_name.clone())
                    .unwrap_or_default();
                let ui = self.clone();
                dialogs::name_category(self, cx, "Rename category", &current, move |name, _cx| {
                    let Some(mut category) = ui
                        .state
                        .borrow()
                        .categories
                        .categories
                        .iter()
                        .find(|c| c.id == channel_id)
                        .cloned()
                    else {
                        return;
                    };
                    category.display_name = name;
                    let client = ui.state.borrow().client.clone();
                    let me = ui.state.borrow().me.id.clone();
                    let team_id = ui.state.borrow().current_team.clone().unwrap_or_default();
                    let ui = ui.clone();
                    runtime::spawn(
                        async move {
                            client
                                .update_categories(&me, &team_id, std::slice::from_ref(&category))
                                .await
                        },
                        move |result, cx| match result {
                            Ok(_) => ui.schedule_sidebar_reload(cx),
                            Err(e) => ui.toast(&format!("Could not rename it: {e}"), cx),
                        },
                    );
                });
            }
            RowAction::DeleteCategory => {
                // The channels in it are not deleted — they fall back to the
                // default category — so this needs no confirmation.
                let ui = self.clone();
                runtime::spawn(
                    async move { client.delete_category(&me, &team_id, &channel_id).await },
                    move |result, cx| match result {
                        Ok(()) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not delete it: {e}"), cx),
                    },
                );
            }
            RowAction::MoveTo(category_id) => {
                // The categories route replaces membership wholesale, so both
                // the old and the new category have to be sent together.
                let mut categories = self.state.borrow().categories.categories.clone();
                for category in categories.iter_mut() {
                    category.channel_ids.retain(|id| id != &channel_id);
                    if category.id == category_id {
                        category.channel_ids.insert(0, channel_id.clone());
                    }
                }
                let ui = self.clone();
                runtime::spawn(
                    async move { client.update_categories(&me, &team_id, &categories).await },
                    move |result, cx| match result {
                        Ok(_) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not move it: {e}"), cx),
                    },
                );
            }
        }
    }

    pub(crate) fn new_category(self: &Rc<Self>, cx: &mut App) {
        let ui = self.clone();
        dialogs::name_category(self, cx, "New category", "", move |name, _cx| {
            let (client, me, team_id) = {
                let st = ui.state.borrow();
                (
                    st.client.clone(),
                    st.me.id.clone(),
                    st.current_team.clone().unwrap_or_default(),
                )
            };
            let category = SidebarCategory {
                display_name: name,
                team_id: team_id.clone(),
                user_id: me.clone(),
                r#type: CategoryType::Custom,
                ..Default::default()
            };
            let ui = ui.clone();
            runtime::spawn(
                async move { client.create_category(&me, &team_id, &category).await },
                move |result, cx| match result {
                    Ok(_) => ui.schedule_sidebar_reload(cx),
                    Err(e) => ui.toast(&format!("Could not create it: {e}"), cx),
                },
            );
        });
    }
}
