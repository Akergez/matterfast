//! Calls-plugin events seen from outside a call: the sidebar marker, the
//! channel banner and the doorbell.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::ChannelType;
use mattermost_calls::CallsEvent as Ev;

use super::calls::participants;
use super::ui::Ui;

impl Ui {
    /// Keeps the sidebar's "call in progress" marker and the channel banner
    /// honest. Joining is a separate, explicit action.
    pub(crate) fn apply_calls_event(self: &Rc<Self>, event: Ev, cx: &mut App) {
        let mut touched = true;
        let mut ring: Option<String> = None;
        let mut stop_ringing = false;
        {
            let mut st = self.state.borrow_mut();
            match event {
                Ev::CallStarted { channel_id, .. } => {
                    // Worth interrupting someone for only where a call is
                    // addressed to them: a DM, or a group they are in. A busy
                    // public channel starting calls all day is not a doorbell.
                    let direct = st.channel(&channel_id).is_some_and(|c| {
                        matches!(c.r#type, ChannelType::Direct | ChannelType::Group)
                    });
                    if direct && st.call.is_none() {
                        ring = Some(channel_id.clone());
                    }
                    st.active_calls.entry(channel_id).or_default();
                }
                Ev::CallEnded { channel_id } => {
                    st.active_calls.remove(&channel_id);
                    // Nothing left to answer.
                    stop_ringing = true;
                }
                // Answered or dismissed on another device. It has to be
                // handled here rather than in the joined-call path: while
                // being rung there is no call of ours, and that path returns
                // early when there is not.
                Ev::UserDismissedNotification { .. } => stop_ringing = true,
                // The server only sends a full roster to the joiner, so the
                // list has to follow the individual comings and goings too —
                // otherwise the banner keeps claiming a call we have left.
                Ev::UserJoined {
                    channel_id,
                    user_id,
                    ..
                } => {
                    let people = st.active_calls.entry(channel_id).or_default();
                    // One person can be in a call from two devices, which is
                    // two sessions but still one face.
                    if !people.contains(&user_id) {
                        people.push(user_id);
                    }
                }
                Ev::UserLeft {
                    channel_id,
                    user_id,
                    ..
                } => {
                    if let Some(people) = st.active_calls.get_mut(&channel_id) {
                        people.retain(|id| id != &user_id);
                        if people.is_empty() {
                            st.active_calls.remove(&channel_id);
                        }
                    }
                }
                Ev::CallState { channel_id, state } => {
                    let people = participants(&state);
                    if people.is_empty() {
                        st.active_calls.remove(&channel_id);
                    } else {
                        st.active_calls.insert(channel_id, people);
                    }
                }
                _ => touched = false,
            }
        }
        if stop_ringing {
            self.stop_ringing(cx);
        }
        if let Some(channel_id) = ring {
            self.ring(&channel_id, cx);
        }
        if touched {
            self.channels.refresh(cx);
            self.refresh_call_ui(cx);
        }
    }
}
