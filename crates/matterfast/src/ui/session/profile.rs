//! Your own profile: name, picture and status.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::account;

impl Ui {
    /// Your own name, nickname, position and picture.
    pub(crate) fn edit_profile(self: &Rc<Self>, cx: &mut App) {
        let (client, me, current) = {
            let st = self.state.borrow();
            let me = &st.me;
            (
                st.client.clone(),
                me.id.clone(),
                (
                    me.first_name.clone(),
                    me.last_name.clone(),
                    me.nickname.clone(),
                    me.position.clone(),
                ),
            )
        };
        let save_ui = self.clone();
        let avatar_ui = self.clone();
        let save_client = client.clone();
        let save_me = me.clone();
        account::edit_profile(
            self,
            cx,
            current,
            move |first, last, nickname, position, _cx| {
                let patch = serde_json::json!({
                    "first_name": first,
                    "last_name": last,
                    "nickname": nickname,
                    "position": position,
                });
                let client = save_client.clone();
                let me = save_me.clone();
                let ui = save_ui.clone();
                runtime::spawn(
                    async move { client.patch_user(&me, &patch).await },
                    move |result, cx| match result {
                        Ok(user) => {
                            ui.state
                                .borrow_mut()
                                .users
                                .insert(user.id.clone(), user.clone());
                            ui.state.borrow_mut().me = user;
                            ui.refresh_all(cx);
                        }
                        Err(e) => ui.toast(&format!("Could not save that: {e}"), cx),
                    },
                );
            },
            move |path, _cx| {
                let client = client.clone();
                let me = me.clone();
                let ui = avatar_ui.clone();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "avatar.png".into());
                runtime::spawn(
                    async move {
                        let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                        client
                            .set_profile_image(&me, &name, bytes)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result, cx| match result {
                        Ok(()) => {
                            // The cached texture is now wrong everywhere it is
                            // drawn, so drop it and let it refetch.
                            let me = ui.state.borrow().me.id.clone();
                            ui.avatars.forget(&me, cx);
                            ui.refresh_all(cx);
                        }
                        Err(e) => ui.toast(&format!("Could not upload that: {e}"), cx),
                    },
                );
            },
        );
    }

    /// The emoji-and-a-line status that shows next to your name.
    pub(crate) fn custom_status(self: &Rc<Self>, cx: &mut App) {
        let (client, current) = {
            let st = self.state.borrow();
            let status = st.me.custom_status();
            (
                st.client.clone(),
                status
                    .map(|s| (s.emoji.clone(), s.text.clone()))
                    .unwrap_or_default(),
            )
        };

        // The last few, which the server keeps as a preference so every client
        // offers the same list.
        let recents: Vec<(String, String)> = self
            .state
            .borrow()
            .preferences
            .iter()
            .find(|p| p.category == "custom_status" && p.name == "recentCustomStatuses")
            .and_then(|p| serde_json::from_str::<Vec<serde_json::Value>>(&p.value).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| {
                let emoji = entry.get("emoji")?.as_str()?.to_string();
                let text = entry.get("text")?.as_str()?.to_string();
                (!emoji.is_empty() || !text.is_empty()).then_some((emoji, text))
            })
            .take(5)
            .collect();

        let set_client = client.clone();
        let set_ui = self.clone();
        let clear_ui = self.clone();
        account::custom_status(
            self,
            cx,
            current,
            recents,
            move |emoji, text, expires_at, _cx| {
                let status = mattermost_api::models::CustomStatus {
                    emoji,
                    text,
                    duration: if expires_at > 0 {
                        "date_and_time".into()
                    } else {
                        String::new()
                    },
                    // RFC3339, unlike every other time in this API — and it
                    // is decoded into a Go time.Time, which insists on the
                    // colon in the zone offset. `format_iso8601` writes
                    // "+0300", which fails to parse and comes back as
                    // "invalid or missing custom_status", so the format is
                    // spelled out.
                    expires_at: (expires_at > 0)
                        .then(|| crate::timefmt::rfc3339_local(expires_at))
                        .flatten(),
                };
                let client = set_client.clone();
                let ui = set_ui.clone();
                runtime::spawn(
                    async move { client.set_custom_status(&status).await },
                    move |result, cx| match result {
                        Ok(()) => ui.reload_me(cx),
                        Err(e) => ui.toast(&format!("Could not set that: {e}"), cx),
                    },
                );
            },
            move |_cx| {
                let client = client.clone();
                let ui = clear_ui.clone();
                runtime::spawn(
                    async move { client.clear_custom_status().await },
                    move |result, cx| match result {
                        Ok(()) => ui.reload_me(cx),
                        Err(e) => ui.toast(&format!("Could not clear that: {e}"), cx),
                    },
                );
            },
        );
    }

    /// Refetches our own user after changing something the server owns the
    /// canonical version of.
    pub(crate) fn reload_me(self: &Rc<Self>, _cx: &mut App) {
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(async move { client.me().await }, move |result, cx| {
            if let Ok(user) = result {
                let mut st = ui.state.borrow_mut();
                st.users.insert(user.id.clone(), user.clone());
                st.me = user;
                drop(st);
                ui.refresh_all(cx);
            }
        });
    }
}
