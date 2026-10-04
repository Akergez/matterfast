//! Keeping the channel list, team list and unread counts current.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::ChannelType;

use super::ui::Ui;
use crate::runtime;

impl Ui {
    /// Refetches the channel list, its memberships and the categories for the
    /// current team.
    ///
    /// Debounced: joining a team, or an admin reorganising channels, produces a
    /// burst of these events, and one reload after the burst is as correct as
    /// nine during it.
    pub(crate) fn schedule_sidebar_reload(self: &Rc<Self>, _cx: &mut App) {
        if self.sidebar_reload_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        runtime::after(std::time::Duration::from_millis(400), move |cx| {
            ui.sidebar_reload_pending.set(false);
            ui.reload_sidebar(cx);
        });
    }

    pub(crate) fn reload_sidebar(self: &Rc<Self>, _cx: &mut App) {
        let (client, team) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        let ui = self.clone();
        runtime::spawn(
            async move {
                tokio::try_join!(
                    client.my_channels(&team_id, false, 0),
                    client.my_channel_members(&team_id),
                    client.sidebar_categories(&team_id),
                )
            },
            move |result, cx| {
                let Ok((channels, members, categories)) = result else {
                    // A failed refresh is not worth interrupting anyone over:
                    // the next event, or the next resync, tries again.
                    return;
                };
                {
                    let mut st = ui.state.borrow_mut();
                    st.channels.clear();
                    st.memberships.clear();
                    for c in channels {
                        st.channels.insert(c.id.clone(), c);
                    }
                    for m in members {
                        st.memberships.insert(m.channel_id.clone(), m);
                    }
                    st.categories = categories;
                }
                ui.channels.refresh(cx);
                ui.refresh_title(cx);
                ui.hydrate_dm_teammates(cx);
            },
        );
    }

    /// Unread and mention counts for every team, so the switcher can show
    /// where something is waiting rather than only what is in front of you.
    pub(crate) fn load_team_unreads(self: &Rc<Self>, _cx: &mut App) {
        let (client, crt) = {
            let st = self.state.borrow();
            (st.client.clone(), st.crt_enabled)
        };
        let ui = self.clone();
        runtime::spawn(
            async move { client.my_team_unreads(crt).await },
            move |result, cx| {
                let Ok(unreads) = result else { return };
                {
                    let mut st = ui.state.borrow_mut();
                    st.team_unreads = unreads
                        .into_iter()
                        .map(|u| (u.team_id, (u.msg_count, u.mention_count)))
                        .collect();
                }
                ui.channels.refresh(cx);
            },
        );
    }

    /// Being added to or removed from a team changes the switcher, not the
    /// channel list.
    pub(crate) fn reload_teams(self: &Rc<Self>, _cx: &mut App) {
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(async move { client.my_teams().await }, move |result, cx| {
            if let Ok(teams) = result {
                ui.state.borrow_mut().teams = teams;
                ui.channels.refresh(cx);
            }
        });
    }

    /// A DM channel carries no display name — just `"<idA>__<idB>"` — so until
    /// the other person is in `users` the sidebar row is blank. Nothing in the
    /// startup sequence fetches them, so do it here: after any load that
    /// replaces the channel list.
    pub(crate) fn hydrate_dm_teammates(self: &Rc<Self>, _cx: &mut App) {
        let (client, ids) = {
            let st = self.state.borrow();
            let ids: Vec<String> = st
                .channels
                .values()
                .filter(|c| c.r#type == ChannelType::Direct)
                .filter_map(|c| c.dm_teammate_id(&st.me.id))
                // Missing either half is a reason to ask: a teammate known
                // from a message may still have no presence.
                .filter(|id| !st.users.contains_key(*id) || !st.statuses.contains_key(*id))
                .map(str::to_string)
                .collect();
            (st.client.clone(), ids)
        };
        if ids.is_empty() {
            return;
        }

        let ui = self.clone();
        runtime::spawn(
            async move {
                (
                    client.users_by_ids(&ids).await.unwrap_or_default(),
                    client.statuses_by_ids(&ids).await.unwrap_or_default(),
                )
            },
            move |(users, statuses), cx| {
                {
                    let mut st = ui.state.borrow_mut();
                    for user in users {
                        st.users.insert(user.id.clone(), user);
                    }
                    st.apply_statuses(statuses);
                }
                ui.channels.refresh(cx);
                ui.refresh_title(cx);
            },
        );
    }
}
