//! The startup sequence: load the account, then go live.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::bootstrap::Bootstrap;
use mattermost_api::ws::{WebSocket, WsUpdate};

use super::calls::participants;
use super::resync::resync;
use super::restore_cache::restore_from_cache;
use super::ui::Ui;
use crate::runtime;
use crate::timefmt::now_ms;
use crate::ui::Action;

/// Runs the startup sequence and connects the websocket.
///
/// `on_auth_failure` is only `Some` for a restored session: one whose token
/// came out of the keyring untested (see `restore_session`, which no longer
/// spends a round trip on `client.me()` before drawing anything). If the boot
/// call below is the one that finds out the token is no good, this is what
/// sends the reader to the login form instead of leaving them looking at a
/// window that will never finish loading. A fresh login has no such fallback
/// to offer — its account was already proven live — so it passes `None`.
pub(crate) fn bootstrap(
    ui: Rc<Ui>,
    on_auth_failure: Option<Box<dyn FnOnce(&mut App)>>,
    _cx: &mut App,
) {
    let client = ui.state.borrow().client.clone();
    restore_from_cache(&ui, client.site_url().to_string());

    runtime::spawn(
        async move {
            let boot = Bootstrap::run(&client, None, None, 0).await?;
            // Calls discovery is optional: the plugin may not be installed.
            let calls = mattermost_calls::discover(&client)
                .await
                .inspect_err(|e| tracing::warn!(error = %e, "calls discovery failed"))
                .ok();
            let active = match &calls {
                Some(_) => mattermost_calls::config::all_channel_states(&client)
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            Ok::<_, mattermost_api::Error>((boot, calls, active))
        },
        move |result, cx| {
            let (boot, calls, active) = match result {
                Ok(v) => v,
                Err(e) => {
                    // A refused token is spent; anything else (server down, no
                    // network) leaves it alone so the next launch can retry —
                    // the same split `restore_session` used to make itself
                    // before this call was the first thing to ask.
                    match (on_auth_failure, &e) {
                        (Some(fallback), e) if !matches!(e, mattermost_api::Error::Http(_)) => {
                            tracing::info!("stored session not usable: {e}");
                            runtime::spawn(crate::session::clear_async(), |_, _| {});
                            crate::cache::clear();
                            fallback(cx);
                        }
                        _ => ui.toast(&format!("Could not load your account: {e}"), cx),
                    }
                    return;
                }
            };

            let ws_url;
            let token;
            let initial_channel;
            {
                let mut st = ui.state.borrow_mut();
                st.config = boot.config;
                st.crt_enabled = boot.crt_enabled;
                st.me = boot.me;
                st.teams = boot.teams;
                st.current_team = boot.initial_team.as_ref().map(|t| t.id.clone());
                for c in boot.channels {
                    st.channels.insert(c.id.clone(), c);
                }
                for m in boot.channel_members {
                    st.memberships.insert(m.channel_id.clone(), m);
                }
                st.categories = boot.categories;
                // Saved posts are preferences, and the startup sequence has
                // already fetched those — asking again would be a second
                // request for something we are holding.
                st.preferences = boot.preferences.clone();
                st.saved_posts = boot
                    .preferences
                    .iter()
                    .filter(|p| p.category == "flagged_post" && p.value == "true")
                    .map(|p| p.name.clone())
                    .collect();
                st.users_fetched_at = now_ms();
                st.calls = calls;
                st.active_calls = active
                    .into_iter()
                    .filter_map(|c| {
                        let call = c.call?;
                        Some((c.channel_id, participants(&call)))
                    })
                    .collect();

                ws_url = st.client.websocket_url();
                token = st.client.token().unwrap_or_default();
                initial_channel = boot.initial_channel.map(|c| c.id);
            }

            ui.refresh_all(cx);
            ui.load_inbox(cx);
            ui.load_drafts(cx);
            ui.load_bots(cx);
            ui.load_custom_emoji(cx);
            ui.load_groups(cx);
            ui.load_team_unreads(cx);
            ui.preload_unread(cx);

            if let Some(id) = initial_channel {
                // The cache may already have put us in this channel. That was
                // a picture of it; arriving is what fetches, marks it read and
                // tells the server, so make sure arriving still happens.
                {
                    let mut st = ui.state.borrow_mut();
                    if st.current_channel.as_deref() == Some(id.as_str()) {
                        st.current_channel = None;
                    }
                }
                ui.dispatch(Action::SelectChannel(id), cx);
            }

            connect_live_updates(&ui, ws_url, token);
        },
    );
}

/// Opens the websocket and routes what it says into the session.
fn connect_live_updates(ui: &Rc<Ui>, ws_url: String, token: String) {
    // Inside the runtime guard: `connect` spawns its own task, and calling
    // that from the main thread panics with "no reactor running".
    let ws = runtime::with_runtime(|| WebSocket::connect(ws_url, token));
    ui.state.borrow_mut().ws = Some(ws.clone());

    let subscription = ws.subscribe();
    runtime::spawn_stream(
        move |tx| async move {
            let mut rx = subscription;
            loop {
                match rx.recv().await {
                    Ok(update) => {
                        if tx.send(update).await.is_err() {
                            break; // the window went away
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        },
        {
            let ui = ui.clone();
            move |update: WsUpdate, cx| match update {
                WsUpdate::Event(event) => ui.apply_event(event, cx),
                WsUpdate::MissedMessages => {
                    // The server could not replay its buffer, so anything
                    // could have changed while we were away.
                    ui.chat.set_connection_problem(None, cx);
                    resync(&ui, cx);
                }
                WsUpdate::Connected { connection_id, .. } => {
                    // Writes carry this from now on, so the server leaves us
                    // out of their echo.
                    ui.state.borrow().client.set_connection_id(connection_id);
                    ui.chat.set_connection_problem(None, cx);
                    // Whatever happened to the groups while the socket was
                    // down was said to nobody.
                    ui.load_groups(cx);
                }
                // Losing the socket is not an event that scrolls past: it
                // stays true until it stops being true, so it is a banner, and
                // it says whether it is being worked on.
                WsUpdate::Disconnected { will_retry, reason } => {
                    let message = if will_retry {
                        "Reconnecting…".to_string()
                    } else {
                        format!("Disconnected: {reason}")
                    };
                    ui.chat.set_connection_problem(Some(&message), cx);
                }
                _ => {}
            }
        },
    );
}
