//! Window title, call controls and right-panel layout, redrawn from state.

use gpui_kit::App;

use super::ui::Ui;
use crate::ui::rhs::PanelMode;

impl Ui {
    /// Mirrors what the desktop app puts in its tray badge: a single mention
    /// count for the whole account.
    pub(crate) fn refresh_title(&self, cx: &mut App) {
        let mentions = self.state.borrow().total_mentions();
        self.mentions.set(mentions);
        let title = if mentions > 0 {
            format!("({mentions}) Matterfast")
        } else {
            "Matterfast".to_string()
        };
        self.window
            .update(cx, move |window, _| window.set_window_title(&title));
    }

    pub(crate) fn refresh_call_ui(&self, cx: &mut App) {
        let st = self.state.borrow();
        let available = st.calls.is_some() && st.current_channel.is_some();
        let reason = if st.calls.is_none() {
            Some("Calls are not enabled on this server")
        } else {
            None
        };
        // The call we are in only shows as "ours" on its own channel; switching
        // away leaves it running, exactly as the web client does — which is why
        // the dock, not the header, carries the controls.
        let in_call = st
            .call
            .as_ref()
            .is_some_and(|c| Some(&c.channel_id) == st.current_channel.as_ref());
        let in_progress = st
            .current_channel
            .as_ref()
            .and_then(|id| st.active_calls.get(id))
            .map(Vec::len);
        drop(st);

        self.chat.set_calls_available(available, reason, cx);
        self.chat.set_call_in_progress(in_progress, cx);
        self.chat.set_in_call(in_call, in_progress.is_some(), cx);
        self.dock.refresh(&self.state, cx);
    }

    /// A thread is a place you read alongside the conversation, so it earns a
    /// static column when there is room. The inbox is a stack you glance at and
    /// dismiss, so it always overlays — pushing the conversation aside for it
    /// would be a heavier gesture than the content deserves.
    pub(crate) fn refresh_panel_mode(&self, cx: &mut App) {
        let overlays = self.narrow.get()
            || matches!(self.right.mode(cx), PanelMode::Inbox | PanelMode::Search(_));
        self.overlay.set_collapsed(overlays, cx);
    }
}
