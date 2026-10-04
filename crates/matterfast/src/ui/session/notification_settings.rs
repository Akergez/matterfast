//! The notification dialogs: per channel and account-wide.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::dialogs;

impl Ui {
    /// Per-channel notification overrides. The dialog is shown with what the
    /// membership currently says, and only what changed is written.
    pub(crate) fn channel_notifications(self: &Rc<Self>, cx: &mut App) {
        let (client, me, channel_id, name, current) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let name = st
                .channel(&channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            let props = st
                .memberships
                .get(&channel_id)
                .map(|m| m.notify_props.clone())
                .unwrap_or_default();
            let desktop = props
                .get("desktop")
                .cloned()
                .unwrap_or_else(|| "default".to_string());
            // "mark_unread: mention" is how Mattermost stores a muted channel,
            // so the switch is the inverse of it.
            let all_activity = props.get("mark_unread").map(String::as_str) != Some("mention");
            let ignore_mentions =
                props.get("ignore_channel_mentions").map(String::as_str) == Some("on");
            (
                st.client.clone(),
                st.me.id.clone(),
                channel_id,
                name,
                (desktop, all_activity, ignore_mentions),
            )
        };

        let ui = self.clone();
        dialogs::channel_notifications(
            self,
            cx,
            &name,
            current,
            move |desktop, all_activity, ignore_mentions, _cx| {
                let mut props = mattermost_api::models::StringMap::new();
                props.insert("desktop".into(), desktop.clone());
                props.insert(
                    "mark_unread".into(),
                    if all_activity { "all" } else { "mention" }.into(),
                );
                props.insert(
                    "ignore_channel_mentions".into(),
                    if ignore_mentions { "on" } else { "off" }.into(),
                );

                let client = client.clone();
                let me = me.clone();
                let channel_id = channel_id.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move {
                        client
                            .set_channel_notify_props(&channel_id, &me, &props)
                            .await
                    },
                    move |result, cx| match result {
                        // The server broadcasts channel_member_updated, which
                        // is what refreshes the muted styling in the sidebar.
                        Ok(()) => ui.schedule_sidebar_reload(cx),
                        Err(e) => ui.toast(&format!("Could not save that: {e}"), cx),
                    },
                );
            },
        );
    }

    /// Account-wide notification settings. These live on the user object's
    /// notify_props, not in preferences — a distinction that trips up most
    /// third-party clients.
    pub(crate) fn account_notifications(self: &Rc<Self>, cx: &mut App) {
        let (client, me, current) = {
            let st = self.state.borrow();
            let props = &st.me.notify_props;
            let desktop = props
                .get("desktop")
                .cloned()
                .unwrap_or_else(|| "mention".to_string());
            let sound = props.get("desktop_sound").map(String::as_str) != Some("false");
            let keys = props.get("mention_keys").cloned().unwrap_or_default();
            let first_name = props.get("first_name").map(String::as_str) == Some("true");
            (
                st.client.clone(),
                st.me.id.clone(),
                (desktop, sound, keys, first_name),
            )
        };

        let ui = self.clone();
        dialogs::account_notifications(
            self,
            cx,
            current,
            move |desktop, sound, keys, first_name, _cx| {
                let patch = serde_json::json!({
                    "notify_props": {
                        "desktop": desktop,
                        "desktop_sound": sound.to_string(),
                        "mention_keys": keys,
                        "first_name": first_name.to_string(),
                    }
                });
                let client = client.clone();
                let me = me.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.patch_user(&me, &patch).await },
                    move |result, cx| match result {
                        Ok(user) => {
                            // These decide every future toast, so the local
                            // copy has to be the server's answer, not ours.
                            ui.state.borrow_mut().me = user;
                        }
                        Err(e) => ui.toast(&format!("Could not save that: {e}"), cx),
                    },
                );
            },
        );
    }
}
