//! The "someone is typing" line and our own typing notifications.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::state::AppState;

impl Ui {
    /// Repaints the "someone is typing" line, and schedules the repaint that
    /// will clear it. The server never says anyone stopped, so the line has to
    /// expire on its own clock.
    pub(crate) fn refresh_typing(self: &Rc<Self>, cx: &mut App) {
        let names = {
            let mut st = self.state.borrow_mut();
            let Some(channel_id) = st.current_channel.clone() else {
                drop(st);
                self.chat.set_typing(&[], cx);
                return;
            };
            let ids = st.typing_in(&channel_id);
            let display = st.teammate_name_display().to_string();
            ids.iter()
                .filter_map(|id| st.users.get(id).map(|u| u.display_name(&display)))
                .collect::<Vec<_>>()
        };
        let anyone = !names.is_empty();
        self.chat.set_typing(&names, cx);

        // One pending sweep at a time, or every keystroke would add a timer.
        if anyone && !self.typing_sweep_pending.replace(true) {
            let ui = self.clone();
            runtime::after(AppState::TYPING_TTL, move |cx| {
                ui.typing_sweep_pending.set(false);
                ui.refresh_typing(cx);
            });
        }
    }

    /// Tells the server we are typing, at most once every few seconds. The
    /// server repeats to other clients on its own schedule, so sending on every
    /// keystroke would be pure noise.
    pub(crate) fn notify_typing(self: &Rc<Self>, _cx: &mut App) {
        if self.typing_sent_recently.replace(true) {
            return;
        }
        {
            let st = self.state.borrow();
            if let (Some(ws), Some(channel)) = (st.ws.as_ref(), st.current_channel.as_ref()) {
                let _ = ws.typing(channel, "");
            }
        }
        let ui = self.clone();
        runtime::after(std::time::Duration::from_secs(3), move |_cx| {
            ui.typing_sent_recently.set(false);
        });
    }
}
