//! The gap-fill after a failed websocket resume.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::Post;

use super::ui::Ui;
use crate::runtime;
use crate::timefmt::now_ms;

pub(crate) fn resync(ui: &Rc<Ui>, _cx: &mut App) {
    let (client, team_id, crt, channel_id, since) = {
        let st = ui.state.borrow();
        let Some(team) = st.current_team.clone() else {
            return;
        };
        let channel = st.current_channel.clone();
        let since = channel
            .as_ref()
            .and_then(|id| st.feeds.get(id))
            .map(|f| f.last_fetched_at)
            .unwrap_or(0);
        (st.client.clone(), team, st.crt_enabled, channel, since)
    };

    let ui = ui.clone();
    runtime::spawn(
        async move {
            let channels = client.my_channels(&team_id, false, 0).await?;
            let members = client.my_channel_members(&team_id).await?;
            let missed = match (&channel_id, since) {
                (Some(id), since) if since > 0 => Some(client.posts_since(id, since, crt).await?),
                _ => None,
            };
            Ok::<_, mattermost_api::Error>((channels, members, channel_id, missed))
        },
        move |result, cx| {
            let Ok((channels, members, channel_id, missed)) = result else {
                return;
            };
            {
                let mut st = ui.state.borrow_mut();
                for c in channels {
                    st.channels.insert(c.id.clone(), c);
                }
                for m in members {
                    st.memberships.insert(m.channel_id.clone(), m);
                }
                if let (Some(id), Some(list)) = (channel_id.clone(), missed) {
                    let posts: Vec<Post> = list.chronological().into_iter().cloned().collect();
                    for post in posts {
                        if post.is_deleted() {
                            if let Some(feed) = st.feeds.get_mut(&id) {
                                feed.remove(&post.id);
                            }
                        } else {
                            // `?since=` reports edits and deletes as well as
                            // new posts, so upsert rather than append.
                            st.apply_post(post);
                        }
                    }
                }
            }
            refresh_changed_users(&ui);

            ui.refresh_all(cx);
            ui.load_inbox(cx);
            // `?since=` is capped by the server, so a long absence can leave a
            // hole between what it returned and now. Scrolling up finds older
            // messages and nothing finds the ones in the middle, so ask for
            // whatever came after the newest post we hold.
            if let Some(id) = channel_id {
                ui.load_newer(id, cx);
            }
        },
    );
}

/// Names, pictures and positions change while we are away, and nothing else
/// tells us: status_change carries presence only.
fn refresh_changed_users(ui: &Rc<Ui>) {
    let (client, ids, since) = {
        let st = ui.state.borrow();
        let ids: Vec<String> = st.users.keys().cloned().collect();
        (st.client.clone(), ids, st.users_fetched_at)
    };
    if ids.is_empty() || since == 0 {
        return;
    }
    let ui = ui.clone();
    runtime::spawn(
        async move { client.users_updated_since(&ids, since).await },
        move |result, cx| {
            let Ok(users) = result else { return };
            if users.is_empty() {
                return;
            }
            {
                let mut st = ui.state.borrow_mut();
                for user in users {
                    // A new picture makes the cached texture stale, exactly as
                    // user_updated does.
                    let changed = st
                        .users
                        .get(&user.id)
                        .is_none_or(|old| old.last_picture_update != user.last_picture_update);
                    if changed {
                        ui.avatars.forget(&user.id, cx);
                    }
                    st.users.insert(user.id.clone(), user);
                }
                st.users_fetched_at = now_ms();
            }
            ui.refresh_all(cx);
        },
    );
}
