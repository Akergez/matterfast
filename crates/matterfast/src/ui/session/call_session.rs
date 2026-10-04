//! Joining and leaving a call, and the toolbar's mute, record, screen and
//! camera toggles.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::App;
use mattermost_calls::{CallSession, CallUpdate, JoinOptions};

use super::ui::Ui;
use crate::audio::AudioIo;
use crate::runtime;
use crate::state::ActiveCall;
use crate::video;

impl Ui {
    /// The call button: join the current channel's call, or hang up.
    pub(crate) fn toggle_call(self: &Rc<Self>, cx: &mut App) {
        let st = self.state.borrow();
        if let Some(call) = &st.call {
            let session = call.session.clone();
            drop(st);
            // Dropping `ActiveCall` stops the microphone; the SFU is told
            // separately, and either way we are out of the call.
            self.close_videos(cx);
            self.state.borrow_mut().call = None;
            self.refresh_call_ui(cx);
            runtime::spawn(async move { session.leave().await }, |_, _| {});
            return;
        }
        let Some(channel_id) = st.current_channel.clone() else {
            return;
        };
        let (Some(discovery), Some(ws)) = (st.calls.clone(), st.ws.clone()) else {
            drop(st);
            self.toast("Calls are not enabled on this server.", cx);
            return;
        };
        let client = st.client.clone();
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move {
                let opts = JoinOptions {
                    channel_id,
                    ..Default::default()
                };
                CallSession::join(&client, ws, opts, &discovery).await
            },
            move |result, cx| match result {
                Ok(session) => ui.call_joined(session, cx),
                Err(e) => ui.toast(&format!("Could not join the call: {e}"), cx),
            },
        );
    }

    /// Starts audio for a freshly joined call and subscribes to its updates.
    fn call_joined(self: &Rc<Self>, session: Arc<CallSession>, cx: &mut App) {
        let audio = match AudioIo::start(session.clone()) {
            Ok(audio) => audio,
            Err(e) => {
                self.toast(&format!("No audio for this call: {e}"), cx);
                runtime::spawn(async move { session.leave().await }, |_, _| {});
                return;
            }
        };
        self.state.borrow_mut().call = Some(ActiveCall {
            channel_id: session.channel_id().to_string(),
            recording: false,
            roster: HashMap::new(),
            speaking: Vec::new(),
            sharing: Vec::new(),
            host_id: String::new(),
            sessions: HashMap::new(),
            muted_users: HashSet::new(),
            hands: Vec::new(),
            screen: None,
            camera: None,
            audio,
            // Mattermost clients join muted, and so does the session itself.
            muted: session.is_muted(),
            session: session.clone(),
        });

        let updates = session.subscribe();
        runtime::spawn_stream(
            move |tx| async move {
                let mut rx = updates;
                loop {
                    match rx.recv().await {
                        Ok(update) => {
                            if tx.send(update).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            },
            {
                let ui = self.clone();
                move |update, cx| ui.apply_call_update(update, cx)
            },
        );

        // The roster arrived before that subscription existed.
        if let Some(state) = session.last_state() {
            self.apply_call_update(CallUpdate::State(state), cx);
        }
        self.refresh_call_ui(cx);
    }

    pub(crate) fn toggle_mute(self: &Rc<Self>, _cx: &mut App) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        let muted = call.muted;
        drop(st);

        let ui = self.clone();
        if muted {
            runtime::spawn(
                async move { session.unmute().await },
                move |result, cx| match result {
                    // The capture loop discards frames until it has this track.
                    Ok(track) => {
                        if let Some(call) = ui.state.borrow().call.as_ref() {
                            call.audio.set_track(track);
                        }
                    }
                    Err(e) => ui.toast(&format!("Could not unmute: {e}"), cx),
                },
            );
        } else {
            runtime::spawn(async move { session.mute().await }, move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not mute: {e}"), cx);
                }
            });
        }
    }

    pub(crate) fn toggle_recording(self: &Rc<Self>, _cx: &mut App) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let (client, channel_id, on) =
            (st.client.clone(), call.channel_id.clone(), !call.recording);
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move { mattermost_calls::set_recording(&client, &channel_id, on).await },
            move |result, cx| {
                if let Err(e) = result {
                    // Only the host may record, and only when the server
                    // allows it at all; both come back as a plain refusal.
                    ui.toast(&format!("Could not change the recording: {e}"), cx);
                }
            },
        );
    }

    pub(crate) fn toggle_screen(self: &Rc<Self>, cx: &mut App) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        if call.screen.is_some() {
            drop(st);
            if let Some(call) = self.state.borrow_mut().call.as_mut() {
                // Dropping the sender stops the encoder and the portal capture.
                call.screen = None;
            }
            self.refresh_call_ui(cx);
            runtime::spawn(async move { session.stop_screen_share().await }, |_, _| {});
            return;
        }
        drop(st);

        let ui = self.clone();
        runtime::spawn(
            async move {
                // The portal picker comes first: there is no point announcing a
                // share the user is about to cancel.
                let (node_id, fd) = video::pick_screen().await?;
                let track = session
                    .start_screen_share()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>((track, node_id, fd))
            },
            move |result, cx| {
                let sender = result.and_then(|(track, node_id, fd)| {
                    video::VideoSender::screen(track, node_id, fd)
                });
                match sender {
                    Ok(sender) => {
                        if let Some(call) = ui.state.borrow_mut().call.as_mut() {
                            call.screen = Some(sender);
                        }
                        ui.refresh_call_ui(cx);
                    }
                    Err(e) => ui.toast(&format!("Could not share the screen: {e}"), cx),
                }
            },
        );
    }

    pub(crate) fn toggle_camera(self: &Rc<Self>, cx: &mut App) {
        let st = self.state.borrow();
        let Some(call) = &st.call else {
            return;
        };
        let session = call.session.clone();
        if call.camera.is_some() {
            drop(st);
            if let Some(call) = self.state.borrow_mut().call.as_mut() {
                call.camera = None;
            }
            self.refresh_call_ui(cx);
            runtime::spawn(async move { session.stop_video().await }, |_, _| {});
            return;
        }
        drop(st);

        let ui = self.clone();
        runtime::spawn(async move { session.start_video().await }, move |result, cx| {
            let sender = result
                .map_err(|e| e.to_string())
                .and_then(video::VideoSender::camera);
            match sender {
                Ok(sender) => {
                    if let Some(call) = ui.state.borrow_mut().call.as_mut() {
                        call.camera = Some(sender);
                    }
                    ui.refresh_call_ui(cx);
                }
                // The server allows camera only on DM channels, and only
                // when EnableVideo is on.
                Err(e) => ui.toast(&format!("Could not start the camera: {e}"), cx),
            }
        });
    }
}
