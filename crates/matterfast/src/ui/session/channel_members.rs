use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::dialogs;

impl Ui {
    /// Who is in this channel, and a way to add or remove people.
    pub(crate) fn channel_members(self: &Rc<Self>, cx: &mut App) {
        let (client, channel_id, name, team_id) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let name = st
                .channel(&channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            (
                st.client.clone(),
                channel_id,
                name,
                st.current_team.clone().unwrap_or_default(),
            )
        };

        let holder: Rc<RefCell<Option<Rc<dialogs::MemberList>>>> = Rc::new(RefCell::new(None));
        let search_holder = holder.clone();
        let search_client = client.clone();
        let search_channel = channel_id.clone();
        let search_ui = self.clone();
        let add_ui = self.clone();
        let add_client = client.clone();
        let add_channel = channel_id.clone();
        let remove_ui = self.clone();
        let remove_channel = channel_id.clone();

        let opened = Rc::new(dialogs::MemberList::present(
            self,
            cx,
            &name,
            move |term, _cx| {
                if term.trim().is_empty() {
                    return;
                }
                let client = search_client.clone();
                let team_id = team_id.clone();
                let channel_id = search_channel.clone();
                let holder = search_holder.clone();
                let state = search_ui.state.clone();
                let asked = term.clone();
                runtime::spawn(
                    // Not-in-channel only: offering someone already here is an
                    // add that does nothing.
                    async move { client.search_users(&term, &team_id, "", &channel_id).await },
                    move |result, cx| {
                        let Ok(users) = result else { return };
                        let display = state.borrow().teammate_name_display().to_string();
                        let rows = users
                            .into_iter()
                            .map(|u| {
                                (
                                    u.id.clone(),
                                    u.display_name(&display),
                                    format!("@{}", u.username),
                                )
                            })
                            .collect();
                        if let Some(list) = holder.borrow().as_ref() {
                            list.set_candidates(&asked, rows, cx);
                        }
                    },
                );
            },
            move |user_id, _cx| {
                let client = add_client.clone();
                let channel_id = add_channel.clone();
                let ui = add_ui.clone();
                runtime::spawn(
                    async move { client.join_channel(&channel_id, &user_id).await },
                    move |result, cx| match result {
                        Ok(_) => ui.toast("Added.", cx),
                        Err(e) => ui.toast(&format!("Could not add them: {e}"), cx),
                    },
                );
            },
            move |user_id, cx| {
                // Removing someone is not undoable and is visible to them, so
                // it asks first.
                let ui = remove_ui.clone();
                let name = ui.user_name(&user_id, cx);
                let client = client.clone();
                let channel_id = remove_channel.clone();
                let confirm_ui = ui.clone();
                dialogs::confirm_remove_member(&confirm_ui, cx, &name, move |_cx| {
                    let client = client.clone();
                    let channel_id = channel_id.clone();
                    let user_id = user_id.clone();
                    let ui = ui.clone();
                    runtime::spawn(
                        async move { client.leave_channel(&channel_id, &user_id).await },
                        move |result, cx| match result {
                            Ok(()) => ui.toast("Removed.", cx),
                            Err(e) => ui.toast(&format!("Could not remove them: {e}"), cx),
                        },
                    );
                });
            },
        ));
        *holder.borrow_mut() = Some(opened.clone());

        // Fill the current members in.
        let ui = self.clone();
        let fill_client = self.state.borrow().client.clone();
        runtime::spawn(
            async move {
                // Two requests rather than three: the users route can filter
                // by channel directly, and the memberships are only needed to
                // find out who the channel admins are.
                let (members, users) = tokio::try_join!(
                    fill_client.channel_members(&channel_id, 0, 200),
                    fill_client.users_in_channel(&channel_id, 0, 200),
                )?;
                Ok::<_, mattermost_api::Error>((members, users))
            },
            move |result, cx| {
                let Ok((members, users)) = result else { return };
                let display = ui.state.borrow().teammate_name_display().to_string();
                let rows = users
                    .into_iter()
                    .map(|user| {
                        let admin = members
                            .iter()
                            .find(|m| m.user_id == user.id)
                            .is_some_and(|m| m.roles.contains("channel_admin"));
                        let name = user.display_name(&display);
                        (user.id, name, format!("@{}", user.username), admin)
                    })
                    .collect();
                opened.set_members(rows, cx);
            },
        );
    }
}
