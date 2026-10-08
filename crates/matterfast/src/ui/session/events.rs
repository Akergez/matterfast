//! Turning a websocket event into state changes and the redraws they need.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::*;
use mattermost_api::ws::Event;

use super::ui::Ui;
use crate::runtime;
use crate::ui::constants::{INBOX_PAGE, REACTION_NOTIFY_PREFIX};
use crate::ui::notify;

/// What an event asks the UI to do once the state borrow is released.
#[derive(Default)]
struct Effects {
    /// A whole-feed rebuild, for the events that really do change every row —
    /// somebody's avatar, a presence dot, a channel switch.
    redraw_messages: bool,
    /// The events that change exactly one post, which is nearly all of them.
    touched: bool,
    redraw_sidebar: bool,
    redraw_typing: bool,
    redraw_draft: bool,
    reload_sidebar: bool,
    reload_teams: bool,
    reload_inbox: bool,
    reload_emoji: bool,
    reload_groups: bool,
    open_dialog: Option<mattermost_api::models::dialog::OpenDialogRequest>,
    notice: Option<String>,
    refetch_post: Option<String>,
    forget_avatar: Option<String>,
    notify_about: Option<mattermost_api::ws::Posted>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::ws::Posted;

    fn posted(at: Millis, root_id: &str) -> Event {
        Event::Posted(Posted {
            post: Post {
                id: format!("post-{at}"),
                channel_id: "c-dev".into(),
                user_id: "u-lena".into(),
                create_at: at,
                root_id: root_id.into(),
                ..Default::default()
            },
            channel_type: "O".into(),
            channel_display_name: String::new(),
            channel_name: String::new(),
            sender_name: String::new(),
            team_id: String::new(),
            mentions: Vec::new(),
            followers: Vec::new(),
            should_ack: false,
        })
    }

    #[test]
    fn a_new_root_in_another_channel_updates_the_inbox_sort_time() {
        let state = crate::demo::state();
        state.borrow_mut().current_channel = Some("c-general".into());
        {
            let mut st = state.borrow_mut();
            let channel = st.channels.get_mut("c-dev").unwrap();
            channel.last_post_at = 100;
            channel.last_root_post_at = 100;
        }
        let (notifier, _) = crate::notifications::Notifier::start();
        let ui = Ui::new(state, notifier);
        let mut fx = Effects::default();
        ui.record_event(posted(200, ""), Some("c-general"), &mut fx);

        assert_eq!(ui.state.borrow().channels["c-dev"].last_root_post_at, 200);
        assert!(fx.redraw_sidebar);
        assert!(!fx.touched, "the open feed did not change");
    }

    #[test]
    fn replies_and_delayed_events_do_not_move_the_last_root_time_backwards() {
        let state = crate::demo::state();
        {
            let mut st = state.borrow_mut();
            let channel = st.channels.get_mut("c-dev").unwrap();
            channel.last_post_at = 200;
            channel.last_root_post_at = 200;
        }
        let (notifier, _) = crate::notifications::Notifier::start();
        let ui = Ui::new(state, notifier);
        ui.record_event(posted(300, "root"), None, &mut Effects::default());
        ui.record_event(posted(100, ""), None, &mut Effects::default());

        let st = ui.state.borrow();
        assert_eq!(st.channels["c-dev"].last_root_post_at, 200);
        assert_eq!(st.channels["c-dev"].last_post_at, 300);
    }
}

impl Ui {
    pub(crate) fn apply_event(self: &Rc<Self>, event: Event, cx: &mut App) {
        // Calls traffic arrives as plugin-namespaced events and never overlaps
        // with the core event set, so it is cheapest to split it off first.
        if let Some(calls) = mattermost_calls::signaling::parse(&event) {
            self.apply_calls_event(calls, cx);
            return;
        }
        if let Event::Other {
            event: name, data, ..
        } = &event
        {
            if let Some(kind) = name.strip_prefix(REACTION_NOTIFY_PREFIX) {
                self.apply_reaction_notice(kind, data, cx);
                return;
            }
            // An LLM answer being written a token at a time.
            if name.strip_prefix(crate::agents::WS_PREFIX) == Some("postupdate") {
                self.apply_stream_update(data, cx);
                return;
            }
        }

        // Reading a message as it lands is not something to be told about, but
        // only while the window is actually in front of the person.
        let focused_channel = self
            .window_is_active(cx)
            .then(|| self.state.borrow().current_channel.clone())
            .flatten();

        let mut fx = Effects::default();
        self.record_event(event, focused_channel.as_deref(), &mut fx);
        self.run_effects(fx, cx);
    }

    /// Applies the event to the state and notes what has to be redrawn.
    fn record_event(&self, event: Event, focused_channel: Option<&str>, fx: &mut Effects) {
        let mut st = self.state.borrow_mut();
        let me = st.me.id.clone();
        let crt = st.crt_enabled;

        match event {
            Event::Posted(posted) => {
                let post = posted.post.clone();
                let channel_id = post.channel_id.clone();
                let is_mine = post.user_id == me;
                let mentions_me = posted.mentions.contains(&me);
                let viewing = st.current_channel.as_deref() == Some(channel_id.as_str());
                let is_reply = post.is_reply();
                let create_at = post.create_at;

                st.apply_post(post.clone());

                // The channel's own counter has to move too, or the unread
                // maths (total − my count) goes negative on the next read.
                if let Some(channel) = st.channels.get_mut(&channel_id) {
                    channel.total_msg_count += 1;
                    if !is_reply {
                        channel.total_msg_count_root += 1;
                        channel.last_root_post_at = channel.last_root_post_at.max(create_at);
                    }
                    channel.last_post_at = channel.last_post_at.max(create_at);
                }
                if let Some(member) = st.memberships.get_mut(&channel_id) {
                    if is_mine || viewing {
                        // Reading it counts as reading it.
                        member.msg_count += 1;
                        if !is_reply {
                            member.msg_count_root += 1;
                        }
                    } else if mentions_me {
                        member.mention_count += 1;
                        if !is_reply || !crt {
                            member.mention_count_root += 1;
                        }
                    }
                }
                if mentions_me && !is_mine {
                    st.mentions.insert(0, post);
                    st.mentions.truncate(INBOX_PAGE as usize);
                }
                if viewing {
                    // Under collapsed threads a reply never enters the feed;
                    // what changes there is the root's reply footer, and only
                    // if the root is on screen at all.
                    fx.touched = true;
                }
                fx.redraw_sidebar = true;
                // Decided here, raised later: the decision needs the state
                // borrow, the toast must not hold it.
                if notify::should_notify(
                    &posted,
                    &st.me,
                    st.memberships.get(&channel_id),
                    focused_channel,
                ) {
                    fx.notify_about = Some(posted);
                }
            }
            // A plugin or slash command asking for a form. Shown outside the
            // state borrow, since it needs the window.
            Event::OpenDialog(request) => fx.open_dialog = Some(*request),
            Event::TeamsChanged => fx.reload_teams = true,
            // The directory is refetched lazily: everyone on screen is already
            // held, and a deactivated user's posts do not vanish.
            Event::UsersChanged => {}
            // Somebody added one: it should be a picture, and on offer,
            // without a restart.
            Event::EmojiChanged => fx.reload_emoji = true,
            // A group is new, gone or renamed, or its people changed.
            Event::GroupsChanged => fx.reload_groups = true,
            Event::Notice { message } => fx.notice = Some(message),
            Event::ThreadsChanged => fx.reload_inbox = true,
            // Nothing on screen depends on these continuously; they matter
            // when one of those windows is open, and it refills on open.
            Event::ListsChanged => {}
            Event::AcknowledgementChanged { post_id } => fx.refetch_post = Some(post_id),
            // Ephemeral posts are shown like any other, but the server will
            // never mention them again — no edit, no delete, and they are
            // gone on the next fetch. That is the intent.
            Event::EphemeralMessage(post) => {
                fx.touched = true;
                st.apply_post(*post);
            }
            Event::PostEdited(post) => {
                fx.touched = true;
                st.apply_post(post);
            }
            Event::PostDeleted(post) => {
                if let Some(feed) = st.feeds.get_mut(&post.channel_id) {
                    feed.remove(&post.id);
                }
                let root = post.thread_root().to_string();
                if let Some(thread) = st.threads.get_mut(&root) {
                    thread.remove(&post.id);
                }
                fx.touched = true;
            }
            Event::ReactionAdded(reaction) => {
                st.apply_reaction(&reaction, true);
                fx.touched = true;
            }
            Event::ReactionRemoved(reaction) => {
                st.apply_reaction(&reaction, false);
                fx.touched = true;
            }
            Event::UserUpdated(user) => {
                // A new picture means the cached texture is stale.
                let changed = st
                    .users
                    .get(&user.id)
                    .map(|old| old.last_picture_update != user.last_picture_update)
                    .unwrap_or(true);
                if changed {
                    fx.forget_avatar = Some(user.id.clone());
                }
                st.users.insert(user.id.clone(), *user);
                fx.redraw_messages = true;
            }
            Event::StatusChange { user_id, status } => {
                let presence = Presence::from(status.as_str());
                // Only redraw when the dot would actually change colour — the
                // server repeats a user's current status often enough that
                // rebuilding the feed each time would be noticeable. Unknown
                // users render as offline already, hence the default rather
                // than an Option compare.
                if st.statuses.insert(user_id, presence).unwrap_or_default() != presence {
                    fx.redraw_messages = true;
                }
            }
            // Another session marked a channel unread; mirror the counters it
            // reported rather than guessing them.
            Event::PostUnread {
                channel_id,
                msg_count,
                mention_count,
                ..
            } => {
                let total = st
                    .channels
                    .get(&channel_id)
                    .map(|c| c.total_msg_count)
                    .unwrap_or(msg_count);
                if let Some(member) = st.memberships.get_mut(&channel_id) {
                    member.msg_count = total - msg_count;
                    member.msg_count_root = member.msg_count;
                    member.mention_count = mention_count;
                    member.mention_count_root = mention_count;
                }
                fx.redraw_sidebar = true;
            }
            Event::Typing {
                channel_id,
                user_id,
                ..
            } => {
                if user_id != me {
                    st.typing_started(channel_id, user_id);
                    fx.redraw_typing = true;
                }
            }
            Event::ChannelsViewed { channel_times } => {
                // Another session read something; mirror it.
                for (channel_id, at) in channel_times {
                    let totals = st
                        .channels
                        .get(&channel_id)
                        .map(|c| (c.total_msg_count, c.total_msg_count_root));
                    if let (Some((total, root)), Some(member)) =
                        (totals, st.memberships.get_mut(&channel_id))
                    {
                        member.last_viewed_at = Some(at);
                        member.msg_count = total;
                        member.msg_count_root = root;
                        member.mention_count = 0;
                        member.mention_count_root = 0;
                    }
                }
                fx.redraw_sidebar = true;
            }
            // A draft written on another device. Our own writes carry a
            // Connection-Id, so the server never echoes these back to us.
            Event::DraftCreated(draft) => {
                if draft.root_id.is_empty() {
                    st.drafts.insert(draft.channel_id, draft.message);
                } else {
                    st.thread_drafts.insert(draft.root_id, draft.message);
                }
                fx.redraw_draft = true;
            }
            Event::DraftDeleted(draft) => {
                if draft.root_id.is_empty() {
                    st.drafts.remove(&draft.channel_id);
                } else {
                    st.thread_drafts.remove(&draft.root_id);
                }
                fx.redraw_draft = true;
            }
            // Everything that can reshape the channel list. Nine events, one
            // answer: ask the server for the list again. A delta per event
            // would be nine chances to drift out of sync with it, and these
            // arrive rarely enough that three requests is cheaper than being
            // wrong.
            Event::ChannelCreated { .. }
            | Event::ChannelUpdated { .. }
            | Event::ChannelDeleted { .. }
            | Event::ChannelMemberUpdated { .. }
            | Event::DirectAdded { .. }
            | Event::PreferencesChanged(_)
            | Event::SidebarCategoriesInvalidated { .. } => fx.reload_sidebar = true,
            // Someone joining a channel only matters to the list when the
            // someone is us.
            Event::UserAdded { user_id, .. } | Event::UserRemoved { user_id, .. }
                if user_id == me =>
            {
                fx.reload_sidebar = true
            }
            Event::AddedToTeam { user_id, .. } | Event::LeaveTeam { user_id, .. }
                if user_id == me =>
            {
                fx.reload_teams = true
            }
            _ => {}
        }
    }

    /// Does what [`record_event`](Self::record_event) asked for, with no
    /// state borrow held.
    fn run_effects(self: &Rc<Self>, mut fx: Effects, cx: &mut App) {
        if let Some(posted) = fx.notify_about.take() {
            let title = if posted.channel_display_name.is_empty() {
                posted.sender_name.clone()
            } else {
                format!("{} — {}", posted.sender_name, posted.channel_display_name)
            };
            self.notify_message(
                &posted.post.channel_id,
                title.trim_start_matches(" — "),
                &notify::body(&posted),
            );
        }
        if fx.reload_teams {
            self.reload_teams(cx);
        }
        if fx.reload_emoji {
            self.load_custom_emoji(cx);
        }
        if fx.reload_groups {
            self.load_groups(cx);
        }
        if fx.reload_inbox {
            self.load_inbox(cx);
        }
        if let Some(request) = fx.open_dialog.take() {
            self.open_dialog(request, cx);
        }
        if let Some(message) = fx.notice.take().filter(|m| !m.is_empty()) {
            self.toast(&message, cx);
        }
        if let Some(post_id) = fx.refetch_post.take() {
            // The event says which post changed but not to what, and
            // acknowledgements live in the post's metadata.
            let client = self.state.borrow().client.clone();
            let ui = self.clone();
            runtime::spawn(async move { client.post(&post_id).await }, move |result, cx| {
                if let Ok(post) = result {
                    ui.state.borrow_mut().apply_post(post);
                    ui.refresh_messages(cx);
                }
            });
        }
        if fx.reload_sidebar {
            self.schedule_sidebar_reload(cx);
        }
        if fx.redraw_typing {
            self.refresh_typing(cx);
        }
        if fx.redraw_draft {
            self.restore_draft(cx);
            self.restore_thread_draft(cx);
            fx.redraw_sidebar = true;
        }

        if let Some(user_id) = fx.forget_avatar.take() {
            self.avatars.forget(&user_id, cx);
        }
        // One post changed, which is nearly every event. The feed is rebuilt
        // from the state and compared with what the list holds, so this costs
        // one row's worth of layout and never moves the reader.
        if fx.redraw_messages || fx.touched {
            self.refresh_messages(cx);
        }
        if fx.redraw_sidebar {
            // The inbox keeps its own ordered rows. A message in another
            // channel does not refresh the open feed, but must reorder these.
            if !fx.redraw_messages && !fx.touched {
                self.right.refresh(&self.state, cx);
            }
            self.channels.refresh(cx);
            self.refresh_title(cx);
        }
    }
}
