//! Reacting to what the SFU tells the joined call.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_calls::protocol::track_type as tt;
use mattermost_calls::{CallUpdate, CallsEvent as Ev};

use super::calls::{is_running, participants};
use super::ui::Ui;
use crate::runtime;
use crate::video;

impl Ui {
    pub(crate) fn apply_call_update(self: &Rc<Self>, update: CallUpdate, cx: &mut App) {
        if self.state.borrow().call.is_none() {
            // We hung up; whatever is still arriving belongs to a call that is
            // no longer ours.
            return;
        }
        match update {
            CallUpdate::RemoteTrack {
                session_id,
                track_type,
                track,
            } => self.remote_track(session_id, track_type, track, cx),
            CallUpdate::Participant(event) => self.apply_participant(event, cx),
            CallUpdate::MuteChanged { muted } => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.muted = muted;
                }
                self.refresh_call_ui(cx);
            }
            CallUpdate::State(state) => {
                let mut st = self.state.borrow_mut();
                if let Some(call) = st.call.as_mut() {
                    call.recording = is_running(state.recording.as_ref());
                    call.host_id = state.host_id.clone();
                    call.sessions = state
                        .sessions
                        .iter()
                        .map(|s| (s.user_id.clone(), s.session_id.clone()))
                        .collect();
                    call.muted_users = state
                        .sessions
                        .iter()
                        .filter(|s| !s.unmuted)
                        .map(|s| s.user_id.clone())
                        .collect();
                    call.hands = state
                        .sessions
                        .iter()
                        .filter(|s| s.raised_hand > 0)
                        .map(|s| (s.raised_hand, s.user_id.clone()))
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .map(|(_, id)| id)
                        .collect();
                    call.roster = state
                        .sessions
                        .iter()
                        .map(|s| (s.session_id.clone(), s.user_id.clone()))
                        .collect();
                    let channel = call.channel_id.clone();
                    st.active_calls.insert(channel, participants(&state));
                }
                drop(st);
                self.refresh_call_ui(cx);
            }
            CallUpdate::Connected(false) | CallUpdate::Ended => {
                self.close_videos(cx);
                self.state.borrow_mut().call = None;
                self.refresh_call_ui(cx);
                self.toast("The call ended.", cx);
            }
            CallUpdate::Error(e) => self.toast(&format!("Call error: {e}"), cx),
            _ => {}
        }
    }

    fn remote_track(
        self: &Rc<Self>,
        session_id: String,
        track_type: String,
        track: std::sync::Arc<mattermost_calls::TrackRemote>,
        cx: &mut App,
    ) {
        match track_type.as_str() {
            // Screen audio is audio like any other; it just happens to come
            // from a share rather than a microphone.
            tt::VOICE | tt::SCREEN_AUDIO => {
                if let Some(call) = self.state.borrow().call.as_ref() {
                    call.audio.play(session_id, track);
                }
            }
            tt::SCREEN | tt::VIDEO => {
                tracing::info!(%session_id, track = %track_type, "a remote video arrived");
                let who = self.speaker_name(&session_id, cx);
                let title = if track_type == tt::SCREEN {
                    format!("{who} is sharing a screen")
                } else {
                    format!("{who} — camera")
                };
                match video::show_remote(&title, track, |cx| cx.refresh_windows()) {
                    Ok(view) => {
                        let key = format!("{session_id}:{track_type}");
                        let mut views = self.video_views.borrow_mut();
                        views.retain(|(id, _)| id != &key);
                        views.push((key, view));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "could not show a remote video");
                        self.toast(&format!("Could not show the video: {e}"), cx)
                    }
                }
            }
            other => tracing::debug!(track = other, "ignoring a remote track"),
        }
    }

    fn apply_participant(self: &Rc<Self>, event: Ev, cx: &mut App) {
        match event {
            // Voice activity and screen shares are what the dock reports, and
            // both arrive per session; the roster turns those into people.
            Ev::UserSpeaking {
                user_id, speaking, ..
            } => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.speaking.retain(|id| id != &user_id);
                    if speaking {
                        call.speaking.insert(0, user_id);
                    }
                }
                self.refresh_call_ui(cx);
            }
            Ev::UserScreenShare {
                user_id,
                session_id,
                sharing,
            } => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.sharing.retain(|id| id != &user_id);
                    if sharing {
                        call.sharing.push(user_id);
                    }
                }
                // A share ending is also the cue to tear its picture down; the
                // track itself may never close.
                if !sharing {
                    self.drop_video(&session_id, tt::SCREEN, cx);
                }
                self.refresh_call_ui(cx);
            }
            Ev::UserDismissedNotification { user_id, .. } => {
                // Answered somewhere else: take the doorbell down here too.
                if user_id == self.state.borrow().me.id {
                    let channel = self.state.borrow().current_channel.clone();
                    if let Some(channel) = channel {
                        self.notifier.withdraw(&crate::ui::notify::call_tag(&channel));
                    }
                }
            }
            Ev::Caption { user_id, text, .. } => {
                let who = self.user_name(&user_id, cx);
                self.dock.set_caption(&format!("{who}:"), &text, cx);
            }
            Ev::UserVideo {
                session_id,
                on: false,
                ..
            } => self.drop_video(&session_id, tt::VIDEO, cx),
            Ev::UserLeft { session_id, .. } => {
                self.drop_video(&session_id, tt::SCREEN, cx);
                self.drop_video(&session_id, tt::VIDEO, cx);
            }
            Ev::JobState { state, .. } => {
                if state.job_type != "recording" {
                    return;
                }
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.recording = is_running(Some(&state));
                }
                self.refresh_call_ui(cx);
            }
            Ev::UserMuted { user_id, muted, .. } => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    if muted {
                        call.muted_users.insert(user_id);
                    } else {
                        call.muted_users.remove(&user_id);
                    }
                }
                self.refresh_call_ui(cx);
            }
            Ev::UserRaisedHand {
                user_id,
                raised_at,
                ..
            } => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.hands.retain(|id| id != &user_id);
                    // Zero means the hand went back down. Appended, not
                    // prepended: the queue is who asked first.
                    if raised_at > 0 {
                        call.hands.push(user_id.clone());
                    }
                }
                self.refresh_call_ui(cx);
            }
            Ev::UserReacted { user_id, reaction, .. } => {
                // In the dock rather than as a toast: a reaction is about the
                // call, and it belongs where the call is. It clears itself,
                // because a reaction is a moment and not a state.
                let who = self.user_name(&user_id, cx);
                let glyph = if reaction.literal.is_empty() {
                    crate::emoji::label(&reaction.name)
                } else {
                    reaction.literal.clone()
                };
                self.dock.set_caption(&who, &glyph, cx);
                let ui = self.clone();
                runtime::after(std::time::Duration::from_secs(4), move |cx| {
                    ui.dock.set_caption("", "", cx);
                });
            }
            host_request => self.apply_host_request(host_request, cx),
        }
    }

    /// Host controls are advisory: the server asks, and the client is what
    /// actually mutes or stops sharing. Ignoring them meant a host muting
    /// someone did nothing at all on their machine.
    fn apply_host_request(self: &Rc<Self>, event: Ev, cx: &mut App) {
        match event {
            Ev::HostMuteRequest { .. } => {
                let muted = self.state.borrow().call.as_ref().is_some_and(|c| c.muted);
                if !muted {
                    self.toggle_mute(cx);
                    self.toast("The host muted you.", cx);
                }
            }
            Ev::HostScreenOffRequest { .. } => {
                let sharing = self
                    .state
                    .borrow()
                    .call
                    .as_ref()
                    .is_some_and(|c| c.screen.is_some());
                if sharing {
                    self.toggle_screen(cx);
                    self.toast("The host stopped your screen share.", cx);
                }
            }
            Ev::HostLowerHandRequest { .. } => {
                let raised = {
                    let st = self.state.borrow();
                    st.call
                        .as_ref()
                        .is_some_and(|c| c.hands.iter().any(|id| id == &st.me.id))
                };
                if raised {
                    self.toggle_hand(cx);
                    self.toast("The host lowered your hand.", cx);
                }
            }
            Ev::HostRemoved { .. } => {
                self.toast("The host removed you from the call.", cx);
                self.toggle_call(cx);
            }
            Ev::HostChanged { host_id, .. } => {
                if let Some(call) = self.state.borrow_mut().call.as_mut() {
                    call.host_id = host_id;
                }
                self.refresh_call_ui(cx);
            }
            _ => {}
        }
    }
}
