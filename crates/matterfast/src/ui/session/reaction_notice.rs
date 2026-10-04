use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;

impl Ui {
    /// Handles the reactions-notify plugin, which tells you when somebody
    /// reacts to something you wrote — something core Mattermost does not.
    ///
    /// The plugin creates no posts: the websocket and its own feed endpoint
    /// are the whole client surface.
    pub(crate) fn apply_reaction_notice(
        self: &Rc<Self>,
        kind: &str,
        data: &mattermost_api::ws::Data,
        cx: &mut App,
    ) {
        let string = |key: &str| {
            data.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match kind {
            "reaction_item" => {
                // The plugin has already applied the push-content policy and
                // decided whether a toast is appropriate — it knows things we
                // do not, like whether you are active in that channel.
                let suppressed = data
                    .get("suppress_desktop")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                self.state.borrow_mut().reaction_unread += 1;
                self.refresh_messages(cx);
                if suppressed {
                    return;
                }
                let channel_id = string("channel_id");
                let title = match string("channel_name").as_str() {
                    "" => format!(
                        ":{}: from {}",
                        string("emoji_name"),
                        string("reactor_username")
                    ),
                    channel => format!("{channel} — :{}:", string("emoji_name")),
                };
                let body = match string("text").as_str() {
                    "" => string("snippet"),
                    text => text.to_string(),
                };
                self.notify_message(&channel_id, &title, &body);
            }
            // Authoritative count, so it replaces ours rather than adjusting it.
            "unread" => {
                let count = data
                    .get("count")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                self.state.borrow_mut().reaction_unread = count;
                self.refresh_messages(cx);
            }
            "item_removed" => {
                let mut st = self.state.borrow_mut();
                st.reaction_unread = (st.reaction_unread - 1).max(0);
                drop(st);
                self.refresh_messages(cx);
            }
            other => tracing::debug!(event = other, "unhandled reactions-notify event"),
        }
    }
}
