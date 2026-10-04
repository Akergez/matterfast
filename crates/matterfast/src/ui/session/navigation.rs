//! Moving between teams, channels and direct messages.

use std::rc::Rc;

use gpui_kit::App;

use super::hydrate::hydrate_authors;
use super::ui::Ui;
use crate::runtime;
use crate::state::ChannelFeed;
use crate::ui::constants::{FOCUS_ON_ARRIVAL, INITIAL_POSTS};
use crate::ui::Action;

impl Ui {
    pub(crate) fn select_team(self: &Rc<Self>, team_id: String, cx: &mut App) {
        // On a collapsed window, picking a team should land on that team's
        // channel list rather than straight into a conversation.
        self.split.set_show_content(false, cx);
        {
            let mut st = self.state.borrow_mut();
            if st.current_team.as_deref() == Some(team_id.as_str()) {
                return;
            }
            st.current_team = Some(team_id.clone());
            st.current_channel = None;
        }

        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                tokio::try_join!(
                    client.my_channels(&team_id, false, 0),
                    client.my_channel_members(&team_id),
                    client.sidebar_categories(&team_id),
                )
            },
            move |result, cx| match result {
                Ok((channels, members, categories)) => {
                    let first = {
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
                        st.sidebar_groups()
                            .into_iter()
                            .flat_map(|(_, cs)| cs)
                            .next()
                            .map(|c| c.id)
                    };
                    ui.refresh_all(cx);
                    ui.load_inbox(cx);
                    ui.load_drafts(cx);
                    ui.load_bots(cx);
                    ui.load_custom_emoji(cx);
                    ui.load_groups(cx);
                    if let Some(id) = first {
                        ui.dispatch(Action::SelectChannel(id), cx);
                    }
                }
                Err(e) => ui.toast(&format!("Could not load that team: {e}"), cx),
            },
        );
    }

    pub(crate) fn select_channel(self: &Rc<Self>, channel_id: String, cx: &mut App) {
        self.split.set_show_content(true, cx);
        self.capture_scroll_anchor(cx);
        // Flush the outgoing channel's draft *before* the composer is pointed
        // at a new one, or the text would be filed under the wrong channel.
        self.save_draft(cx);
        {
            let mut st = self.state.borrow_mut();
            if st.current_channel.as_deref() == Some(channel_id.as_str()) {
                return;
            }
            st.current_channel = Some(channel_id.clone());
        }
        self.refresh_messages(cx);
        if let Some(anchor) = self.state.borrow().scroll_anchors.get(&channel_id).cloned() {
            self.chat.restore_anchor(&anchor, cx);
        }
        self.refresh_call_ui(cx);
        self.refresh_typing(cx);
        self.restore_draft(cx);
        self.schedule_snapshot(cx);

        let (client, crt, have_feed) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.crt_enabled,
                st.feeds.contains_key(&channel_id),
            )
        };

        self.chat.set_loading(!have_feed, cx);
        // A channel with unread messages opens *at* them rather than at the
        // newest post — the same fetch the other clients use, so the page
        // arrives centred on where reading stopped instead of needing a scroll
        // back to find it.
        let unread = self.state.borrow().unread(&channel_id).is_unread();

        if !have_feed {
            self.load_first_page(&client, &channel_id, unread, crt, cx);
        } else {
            if FOCUS_ON_ARRIVAL {
                self.chat.focus_composer(cx);
            }
            // A feed that came out of the cache is last time's picture: it
            // is worth showing at once and wrong until it has been topped up.
            let behind = self
                .state
                .borrow()
                .feeds
                .get(&channel_id)
                .is_some_and(|feed| !feed.at_latest);
            if behind {
                self.load_newer(channel_id.clone(), cx);
            }
        }

        // How many people are here. Cheap, cached by the server, and the
        // question "who can see this" comes up constantly in a channel you
        // have just walked into.
        {
            let stats_client = client.clone();
            let id = channel_id.clone();
            let ui = self.clone();
            runtime::spawn(
                async move { stats_client.channel_stats(&id).await },
                move |result, cx| {
                    if let Ok(stats) = result {
                        ui.chat.set_member_count(Some(stats.member_count), cx);
                    }
                },
            );
        }

        // Tell the server we are looking at this channel so read state syncs to
        // our other sessions.
        let id = channel_id.clone();
        let view_client = client.clone();
        runtime::spawn(
            async move { view_client.view_channel(&id, "", crt).await },
            |_, _| {},
        );

        // Locally zero the unread counters so the sidebar reacts immediately.
        {
            let mut st = self.state.borrow_mut();
            let total = st
                .channels
                .get(&channel_id)
                .map(|c| (c.total_msg_count, c.total_msg_count_root));
            if let (Some((total, total_root)), Some(member)) =
                (total, st.memberships.get_mut(&channel_id))
            {
                member.msg_count = total;
                member.msg_count_root = total_root;
                member.mention_count = 0;
                member.mention_count_root = 0;
                member.urgent_mention_count = 0;
            }
        }
        self.channels.refresh(cx);
        self.refresh_title(cx);
    }

    /// Fetches a channel's first page when nothing of it is held yet.
    fn load_first_page(
        self: &Rc<Self>,
        client: &mattermost_api::Client,
        channel_id: &str,
        unread: bool,
        crt: bool,
        cx: &mut App,
    ) {
        // Repaint now that the pane knows it is waiting; the fetch below
        // may take a while and the reader should not be looking at "this
        // is the beginning of…" in the meantime.
        self.refresh_messages(cx);
        let id = channel_id.to_string();
        let fetch_client = client.clone();
        let ui = self.clone();
        let channel_id = channel_id.to_string();
        runtime::spawn(
            async move {
                let posts = if unread {
                    fetch_client
                        .posts_around_unread(&id, INITIAL_POSTS / 2, INITIAL_POSTS / 2, crt)
                        .await?
                } else {
                    fetch_client
                        .posts_for_channel(&id, 0, INITIAL_POSTS, crt)
                        .await?
                };
                let (authors, statuses) = hydrate_authors(&fetch_client, &posts).await;
                Ok::<_, mattermost_api::Error>((posts, authors, statuses))
            },
            move |result, cx| match result {
                Ok((posts, authors, statuses)) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        for user in authors {
                            st.users.insert(user.id.clone(), user);
                        }
                        st.apply_statuses(statuses);
                        st.feeds
                            .insert(channel_id.clone(), ChannelFeed::from_list(&posts));
                    }
                    ui.chat.set_loading(false, cx);
                    ui.refresh_messages(cx);
                    if let Some(anchor) = ui.state.borrow().scroll_anchors.get(&channel_id).cloned()
                    {
                        ui.chat.restore_anchor(&anchor, cx);
                    }
                    if FOCUS_ON_ARRIVAL {
                        ui.chat.focus_composer(cx);
                    }
                    ui.resolve_mentions(cx);
                    // Straight into the store, so the next launch has
                    // this channel without asking for it again.
                    ui.store_posts(posts.chronological().into_iter().cloned().collect(), cx);
                }
                Err(e) => {
                    ui.chat.set_loading(false, cx);
                    ui.refresh_messages(cx);
                    ui.toast(&format!("Could not load messages: {e}"), cx);
                }
            },
        );
    }

    pub(crate) fn open_direct_message(self: &Rc<Self>, user_id: String, _cx: &mut App) {
        let (client, me) = {
            let st = self.state.borrow();
            (st.client.clone(), st.me.id.clone())
        };
        let ui = self.clone();
        runtime::spawn(
            async move { client.create_direct_channel(&me, &user_id).await },
            move |result, cx| match result {
                Ok(channel) => {
                    let id = channel.id.clone();
                    ui.state.borrow_mut().channels.insert(id.clone(), channel);
                    ui.channels.refresh(cx);
                    ui.dispatch(Action::SelectChannel(id), cx);
                }
                Err(e) => ui.toast(&format!("Could not open that conversation: {e}"), cx),
            },
        );
    }
}
