use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::dialogs;

impl Ui {
    pub(crate) fn leave_channel(self: &Rc<Self>, cx: &mut App) {
        let (client, me, channel_id, name) = {
            let st = self.state.borrow();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let name = st
                .channel(&channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_default();
            (st.client.clone(), st.me.id.clone(), channel_id, name)
        };

        let ui = self.clone();
        dialogs::confirm_leave(self, cx, &name, move |_cx| {
            let client = client.clone();
            let me = me.clone();
            let channel_id = channel_id.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.leave_channel(&channel_id, &me).await },
                move |result, cx| match result {
                    Ok(()) => {
                        ui.state.borrow_mut().current_channel = None;
                        ui.schedule_sidebar_reload(cx);
                        ui.refresh_messages(cx);
                    }
                    Err(e) => ui.toast(&format!("Could not leave: {e}"), cx),
                },
            );
        });
    }
}
