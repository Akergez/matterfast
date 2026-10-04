//! Sending messages and reacting to them.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::{Post, Reaction};

use super::ui::Ui;
use crate::runtime;
use crate::timefmt::{now_ms, unique};

impl Ui {
    pub(crate) fn send_message(
        self: &Rc<Self>,
        text: String,
        root_id: Option<String>,
        cx: &mut App,
    ) {
        let (client, channel_id, me, file_ids) = {
            let mut st = self.state.borrow_mut();
            let channel = match &root_id {
                // A reply belongs to the root's channel, which is not
                // necessarily the one on screen.
                Some(root) => st
                    .find_post(root)
                    .map(|p| p.channel_id.clone())
                    .or_else(|| st.current_channel.clone()),
                None => st.current_channel.clone(),
            };
            let Some(id) = channel else { return };
            // Attachments leave the queue with the message they go out on.
            let files: Vec<String> = st.pending_files.drain(..).map(|(id, _)| id).collect();
            (st.client.clone(), id, st.me.id.clone(), files)
        };
        // An empty message with nothing attached is not a message.
        if text.trim().is_empty() && file_ids.is_empty() {
            return;
        }
        // A slash command is an instruction to the server, not a message. It
        // was being posted as literal text, which is how "/away" ended up in
        // channels as a joke about the client.
        if text.starts_with('/') && !text.starts_with("//") && file_ids.is_empty() {
            self.run_command(channel_id, text, cx);
            return;
        }
        let priority = self.chat.priority(cx);
        self.chat.reset_priority(cx);
        self.refresh_attachments(cx);

        // Show it immediately. The websocket echo replaces this copy — matched
        // on `pending_post_id` — so a slow round trip never looks like a
        // dropped message.
        let pending_id = format!("pending{}", unique());
        let optimistic = Post {
            id: pending_id.clone(),
            pending_post_id: pending_id.clone(),
            channel_id: channel_id.clone(),
            user_id: me,
            message: text.clone(),
            root_id: root_id.clone().unwrap_or_default(),
            create_at: now_ms(),
            file_ids: file_ids.clone(),
            ..Default::default()
        };
        self.state.borrow_mut().apply_post(optimistic.clone());
        self.refresh_messages(cx);
        // Both send paths — the channel composer and the thread panel's —
        // come through here, so the rule that your own message brings you to
        // the bottom lives here rather than at either call site.
        self.chat.follow_own_post(&optimistic, &self.state, cx);

        let ui = self.clone();
        let reply_to = root_id.clone();
        let placeholder_id = pending_id.clone();
        runtime::spawn(
            async move {
                let mut post = Post {
                    channel_id,
                    message: text,
                    root_id: reply_to.unwrap_or_default(),
                    pending_post_id: pending_id,
                    file_ids,
                    ..Default::default()
                };
                if !priority.is_empty() {
                    // Priority travels in the post's metadata, which the
                    // server reads on create and echoes back on the result.
                    post.metadata.get_or_insert_with(Default::default).priority =
                        Some(mattermost_api::models::PostPriority {
                            priority: Some(priority),
                            requested_ack: None,
                            persistent_notifications: None,
                        });
                }
                client.create_post(&post).await
            },
            move |result, cx| match result {
                Ok(post) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        // Retire the optimistic copy explicitly rather than
                        // trusting the response to carry `pending_post_id`
                        // back — otherwise a server that drops it leaves the
                        // message on screen twice.
                        st.remove_post(&placeholder_id);
                        st.apply_post(post);
                    }
                    ui.refresh_messages(cx);
                }
                Err(e) => {
                    ui.toast(&format!("Message not sent: {e}"), cx);
                    // Take the optimistic copy back down: leaving it there
                    // would claim the message was sent.
                    ui.state.borrow_mut().remove_post(&placeholder_id);
                    ui.refresh_messages(cx);
                }
            },
        );
    }

    pub(crate) fn toggle_reaction(self: &Rc<Self>, post_id: String, emoji: String, cx: &mut App) {
        let (client, me, currently_mine) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.me.id.clone(),
                st.has_my_reaction(&post_id, &emoji),
            )
        };

        // Optimistic, like the send path: a reaction is the one interaction
        // where a round trip is very visible.
        let reaction = Reaction {
            user_id: me.clone(),
            post_id: post_id.clone(),
            emoji_name: emoji.clone(),
            ..Default::default()
        };
        self.state
            .borrow_mut()
            .apply_reaction(&reaction, !currently_mine);
        self.refresh_messages(cx);

        let ui = self.clone();
        let request_post = post_id.clone();
        let request_emoji = emoji.clone();
        runtime::spawn(
            async move {
                if currently_mine {
                    client
                        .remove_reaction(&me, &request_post, &request_emoji)
                        .await
                } else {
                    client
                        .add_reaction(&me, &request_post, &request_emoji)
                        .await
                        .map(|_| ())
                }
            },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("Reaction failed: {e}"), cx);
                    // Put it back the way it was.
                    ui.state
                        .borrow_mut()
                        .apply_reaction(&reaction, currently_mine);
                    ui.refresh_messages(cx);
                }
            },
        );
    }
}
