//! Desktop notifications: raising them, withdrawing them and what pressing
//! one does.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::ChannelType;

use super::ui::Ui;
use crate::notifications::Notice;
use crate::runtime;
use crate::ui::{notify, shell, Action};

impl Ui {
    /// Raises a desktop notification for a message, tagged by its channel.
    pub(crate) fn notify_message(&self, channel_id: &str, title: &str, body: &str) {
        self.notifier.show(Notice {
            tag: notify::message_tag(channel_id),
            title: title.to_string(),
            body: body.to_string(),
            urgent: false,
            actions: Vec::new(),
        });
    }

    /// Something was pressed on one of our notifications.
    pub(crate) fn notification_pressed(
        self: &Rc<Self>,
        tag: &str,
        action: Option<&str>,
        cx: &mut App,
    ) {
        match (notify::pressed(tag), action) {
            (Some(notify::Pressed::Call(channel_id)), Some(notify::DISMISS_CALL)) => {
                self.stop_ringing(cx);
                // In a DM there is one person waiting, and declining tells
                // them; anywhere else there is nobody to tell, so it is only
                // silenced for us and our other devices.
                let (client, direct) = {
                    let st = self.state.borrow();
                    let direct = st
                        .channel(channel_id)
                        .is_some_and(|c| matches!(c.r#type, ChannelType::Direct));
                    (st.client.clone(), direct)
                };
                let channel_id = channel_id.to_string();
                runtime::spawn(
                    async move {
                        if direct {
                            mattermost_calls::decline(&client, &channel_id).await
                        } else {
                            mattermost_calls::dismiss_notification(&client, &channel_id).await
                        }
                    },
                    |_, _| {},
                );
            }
            (Some(notify::Pressed::Call(channel_id)), Some(notify::JOIN_CALL)) => {
                self.stop_ringing(cx);
                shell::present(cx);
                self.dispatch(Action::SelectChannel(channel_id.to_string()), cx);
                self.dispatch(Action::ToggleCall, cx);
            }
            // The notification itself: go and look.
            (Some(notify::Pressed::Call(channel_id) | notify::Pressed::Message(channel_id)), _) => {
                shell::present(cx);
                self.dispatch(Action::SelectChannel(channel_id.to_string()), cx);
            }
            (None, _) => {}
        }
    }

    /// Announces an incoming call, with the two things you might want to do
    /// about it. Mattermost has no ring signal of its own — a call starting is
    /// the whole event — so this is the client's doing.
    pub(crate) fn ring(self: &Rc<Self>, channel_id: &str, _cx: &mut App) {
        let title = {
            let st = self.state.borrow();
            st.channel(channel_id)
                .map(|c| st.channel_title(c))
                .unwrap_or_else(|| "Someone".to_string())
        };
        // Tagged by channel, so a second call in the same DM replaces the
        // first rather than stacking two doorbells.
        self.notifier.show(Notice {
            tag: notify::call_tag(channel_id),
            title: format!("{title} is calling"),
            body: "Incoming call".to_string(),
            urgent: true,
            actions: vec![
                (notify::JOIN_CALL.to_string(), "Join".to_string()),
                (notify::DISMISS_CALL.to_string(), "Dismiss".to_string()),
            ],
        });
        self.state.borrow_mut().ringing.push(channel_id.to_string());
    }

    /// Takes every incoming-call notification back down.
    pub(crate) fn stop_ringing(&self, _cx: &mut App) {
        let ringing = std::mem::take(&mut self.state.borrow_mut().ringing);
        for channel_id in ringing {
            self.notifier.withdraw(&notify::call_tag(&channel_id));
        }
    }
}
