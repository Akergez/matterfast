//! Things a participant can do in a call: host controls and raising a hand.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::call_dock::HostAction;

impl Ui {
    /// Applies a host control. These are HTTP routes rather than websocket
    /// messages — the one part of the calls protocol that is.
    pub(crate) fn host_control(
        self: &Rc<Self>,
        session_id: String,
        what: HostAction,
        _cx: &mut App,
    ) {
        let Some(session) = self.state.borrow().call.as_ref().map(|c| c.session.clone()) else {
            return;
        };
        let ui = self.clone();
        runtime::spawn(
            async move {
                match what {
                    HostAction::MuteOthers => session.host_mute_others().await,
                    HostAction::EndCall => session.host_end_call().await,
                    HostAction::Mute => session.host_mute(&session_id).await,
                    HostAction::StopSharing => session.host_screen_off(&session_id).await,
                    HostAction::LowerHand => session.host_lower_hand(&session_id).await,
                    HostAction::MakeHost => session.host_make(&session_id).await,
                    HostAction::Remove => session.host_remove(&session_id).await,
                }
            },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not do that: {e}"), cx);
                }
            },
        );
    }

    /// Raises or lowers your own hand. The SFU echoes it back as
    /// `user_raise_hand`, which is what actually updates the roster.
    pub(crate) fn toggle_hand(self: &Rc<Self>, _cx: &mut App) {
        let (session, raise) = {
            let st = self.state.borrow();
            let Some(call) = st.call.as_ref() else { return };
            (
                call.session.clone(),
                !call.hands.iter().any(|id| id == &st.me.id),
            )
        };
        let ui = self.clone();
        runtime::spawn(
            async move { session.raise_hand(raise).await },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not do that: {e}"), cx);
                }
            },
        );
    }
}
