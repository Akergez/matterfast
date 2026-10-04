//! Drawing last launch's picture from the local store before the network
//! answers.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::state::ChannelFeed;
use crate::ui::constants::INITIAL_POSTS;

/// Everything here is replaced the moment the real data lands; it is on screen
/// so that launching the app shows your channels instead of an empty window
/// for the length of a round trip.
pub(crate) fn restore_from_cache(ui: &Rc<Ui>, server: String) {
    let pointer = crate::cache::load(&server);
    let ui = ui.clone();
    runtime::spawn(
        async move {
            let store = crate::store::Store::open(&server).await.ok()?;
            let (channels, members) = store.channels().await.ok()?;
            let users = store.users().await.ok()?;
            Some((store, channels, members, users))
        },
        move |loaded, cx| {
            let Some((store, channels, members, users)) = loaded else {
                return;
            };
            *ui.store.borrow_mut() = Some(store.clone());
            {
                let mut st = ui.state.borrow_mut();
                for channel in channels {
                    st.channels.entry(channel.id.clone()).or_insert(channel);
                }
                for member in members {
                    st.memberships
                        .entry(member.channel_id.clone())
                        .or_insert(member);
                }
                for user in users {
                    st.users.entry(user.id.clone()).or_insert(user);
                }
                if let Some(pointer) = &pointer {
                    // Only if the live data has not already answered.
                    if st.current_team.is_none() {
                        st.current_team = pointer.current_team.clone();
                    }
                    st.scroll_anchors.clone_from(&pointer.scroll_anchors);
                }
            }
            ui.refresh_all(cx);

            // And the messages for wherever we were, so the conversation is
            // there too rather than just the list around it.
            if let Some(channel_id) = pointer
                .as_ref()
                .and_then(|p| p.current_channel.clone())
                .filter(|id| ui.state.borrow().current_channel.as_deref() != Some(id.as_str()))
            {
                let anchor = pointer
                    .as_ref()
                    .and_then(|p| p.scroll_anchors.get(&channel_id).cloned());
                restore_channel(&ui, store, channel_id, anchor);
            }
        },
    );
}

fn restore_channel(
    ui: &Rc<Ui>,
    store: crate::store::Store,
    channel_id: String,
    anchor: Option<String>,
) {
    let ui = ui.clone();
    runtime::spawn(
        async move {
            let posts = if let Some(post_id) = anchor.as_deref() {
                let around = store.posts_around(&channel_id, post_id, 10, 10).await;
                match around {
                    Ok(posts) if !posts.is_empty() => Ok(posts),
                    Ok(_) => store.posts(&channel_id, INITIAL_POSTS as usize).await,
                    Err(error) => Err(error),
                }
            } else {
                store.posts(&channel_id, INITIAL_POSTS as usize).await
            };
            (channel_id, anchor, posts)
        },
        move |(channel_id, anchor, posts), cx: &mut App| {
            let Ok(posts) = posts else { return };
            if posts.is_empty() {
                return;
            }
            {
                let mut st = ui.state.borrow_mut();
                st.feeds
                    .entry(channel_id.clone())
                    .or_insert_with(|| ChannelFeed::from_posts(posts));
                if st.current_channel.is_none() {
                    st.current_channel = Some(channel_id);
                }
            }
            ui.refresh_messages(cx);
            if let Some(anchor) = anchor {
                ui.chat.restore_anchor(&anchor, cx);
            }
        },
    );
}
