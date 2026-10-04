//! Creating, browsing, renaming and archiving channels.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::ChannelType;

use super::ui::Ui;
use crate::runtime;
use crate::ui::{dialogs, Action};

impl Ui {
    pub(crate) fn new_channel(self: &Rc<Self>, cx: &mut App) {
        let ui = self.clone();
        dialogs::create_channel(self, cx, move |display_name, url, purpose, private, _cx| {
            let (client, team) = {
                let st = ui.state.borrow();
                (st.client.clone(), st.current_team.clone())
            };
            let Some(team_id) = team else { return };
            let kind = if private {
                ChannelType::Private
            } else {
                ChannelType::Open
            };
            let ui = ui.clone();
            let purpose = purpose.clone();
            runtime::spawn(
                async move {
                    let channel = client
                        .create_channel(&team_id, &url, &display_name, kind)
                        .await?;
                    // Purpose is a separate patch; a create that succeeds and
                    // a purpose that does not is still a usable channel.
                    if !purpose.is_empty() {
                        let _ = client.update_channel_header(&channel.id, &purpose).await;
                    }
                    Ok::<_, mattermost_api::Error>(channel)
                },
                move |result, cx| match result {
                    Ok(channel) => {
                        ui.schedule_sidebar_reload(cx);
                        ui.dispatch(Action::SelectChannel(channel.id), cx);
                    }
                    Err(e) => ui.toast(&format!("Could not create it: {e}"), cx),
                },
            );
        });
    }

    pub(crate) fn browse_channels(self: &Rc<Self>, cx: &mut App) {
        let browser = Rc::new(RefCell::new(None::<Rc<dialogs::ChannelBrowser>>));
        let ui = self.clone();
        let search_ui = self.clone();
        let holder = browser.clone();

        let opened = Rc::new(dialogs::ChannelBrowser::present(
            self,
            cx,
            move |term, _cx| {
                let (client, team) = {
                    let st = search_ui.state.borrow();
                    (st.client.clone(), st.current_team.clone())
                };
                let Some(team_id) = team else { return };
                let holder = holder.clone();
                let state = search_ui.state.clone();
                let asked = term.clone();
                runtime::spawn(
                    async move {
                        // An empty box means "show me what is there", which is
                        // the browse list rather than a search for nothing.
                        if term.trim().is_empty() {
                            client.channels_for_team(&team_id, 0, 100).await
                        } else {
                            client.search_channels(&team_id, &term).await
                        }
                    },
                    move |result, cx| {
                        let Ok(channels) = result else { return };
                        let joined = &state.borrow().channels;
                        let rows = channels
                            .into_iter()
                            .filter(|c| c.delete_at == 0)
                            .map(|c| {
                                let member = joined.contains_key(&c.id);
                                (c.id, c.display_name, c.purpose, member)
                            })
                            .collect();
                        if let Some(browser) = holder.borrow().as_ref() {
                            browser.set_results(&asked, rows, cx);
                        }
                    },
                );
            },
            move |channel_id, _cx| {
                let (client, me) = {
                    let st = ui.state.borrow();
                    (st.client.clone(), st.me.id.clone())
                };
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.join_channel(&channel_id, &me).await },
                    move |result, cx| match result {
                        Ok(member) => {
                            ui.schedule_sidebar_reload(cx);
                            ui.dispatch(Action::SelectChannel(member.channel_id), cx);
                        }
                        Err(e) => ui.toast(&format!("Could not join: {e}"), cx),
                    },
                );
            },
        ));
        // The browser asks for its first page as it opens, and that request
        // needs the handle to fill — which only exists once present returns.
        // Storing it here is what closes that loop.
        *browser.borrow_mut() = Some(opened);
    }

    /// The channel's name and topic.
    pub(crate) fn edit_channel(self: &Rc<Self>, cx: &mut App) {
        let (client, channel_id, current) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let Some(channel) = st.channel(&channel_id) else {
                return;
            };
            (
                st.client.clone(),
                channel_id,
                (channel.display_name.clone(), channel.header.clone()),
            )
        };

        let ui = self.clone();
        dialogs::edit_channel(self, cx, current, move |name, header, _cx| {
            let client = client.clone();
            let channel_id = channel_id.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move {
                    // Two routes, because the server patches these separately.
                    if !name.is_empty() {
                        client.rename_channel(&channel_id, &name).await?;
                    }
                    client.update_channel_header(&channel_id, &header).await
                },
                move |result, cx| match result {
                    // channel_updated comes back over the socket and repaints.
                    Ok(_) => {}
                    Err(e) => ui.toast(&format!("Could not save that: {e}"), cx),
                },
            );
        });
    }

    pub(crate) fn archive_channel(self: &Rc<Self>, cx: &mut App) {
        let (client, channel_id, name) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let name = st
                .channel(&channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            (st.client.clone(), channel_id, name)
        };

        let ui = self.clone();
        dialogs::confirm_archive(self, cx, &name, move |_cx| {
            let client = client.clone();
            let channel_id = channel_id.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.archive_channel(&channel_id).await },
                move |result, cx| match result {
                    Ok(()) => {
                        ui.state.borrow_mut().current_channel = None;
                        ui.schedule_sidebar_reload(cx);
                        ui.refresh_messages(cx);
                    }
                    Err(e) => ui.toast(&format!("Could not archive it: {e}"), cx),
                },
            );
        });
    }
}
