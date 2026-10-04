use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;

impl Ui {
    /// Sets your own presence, showing it immediately: the server echoes it
    /// back as a status_change, but the click should not wait for a round trip
    /// to look like it landed.
    pub(crate) fn set_status(self: &Rc<Self>, status: String, cx: &mut App) {
        let (client, me) = {
            let mut st = self.state.borrow_mut();
            let me = st.me.id.clone();
            let presence = mattermost_api::models::Presence::from(status.as_str());
            st.statuses.insert(me.clone(), presence);
            (st.client.clone(), me)
        };
        self.channels.refresh(cx);
        self.refresh_messages(cx);

        let ui = self.clone();
        runtime::spawn(
            async move { client.set_status(&me, &status).await },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Could not change your status: {e}"), cx);
                }
            },
        );
    }
}
