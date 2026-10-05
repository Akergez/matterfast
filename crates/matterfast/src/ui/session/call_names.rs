//! Remote pictures on screen and the names the call UI shows for people.

use gpui_kit::App;

use super::ui::Ui;
use crate::ui::Part;

impl Ui {
    /// Removes one remote picture, if it is on screen.
    pub(crate) fn drop_video(&self, session_id: &str, kind: &str, cx: &mut App) {
        let key = format!("{session_id}:{kind}");
        self.video_views.borrow_mut().retain(|(id, _)| id != &key);
        crate::ui::redraw(&[Part::Frame], cx);
    }

    pub(crate) fn close_videos(&self, cx: &mut App) {
        self.video_views.borrow_mut().clear();
        crate::ui::redraw(&[Part::Frame], cx);
    }

    /// A person's display name, or something honest when we do not have them.
    pub(crate) fn user_name(&self, user_id: &str, _cx: &mut App) -> String {
        let st = self.state.borrow();
        st.users
            .get(user_id)
            .map(|user| st.display_name(user))
            .unwrap_or_else(|| "Someone".to_string())
    }

    /// Who a media session belongs to, as far as the roster knows.
    pub(crate) fn speaker_name(&self, session_id: &str, _cx: &mut App) -> String {
        let st = self.state.borrow();
        st.call
            .as_ref()
            .and_then(|call| call.roster.get(session_id))
            .and_then(|user_id| st.users.get(user_id))
            .map(|user| st.display_name(user))
            .unwrap_or_else(|| "Someone".to_string())
    }
}
