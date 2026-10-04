//! The Agents plugin: which bots exist, streamed answers and "catch me up".

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::Action;

impl Ui {
    /// Applies a streamed LLM answer.
    ///
    /// `next` carries the whole message so far rather than the new part, so
    /// this replaces the text instead of appending — appending would double
    /// every character.
    pub(crate) fn apply_stream_update(
        self: &Rc<Self>,
        data: &mattermost_api::ws::Data,
        cx: &mut App,
    ) {
        use crate::agents::StreamUpdate;
        match crate::agents::parse_stream(data) {
            StreamUpdate::Text { post_id, message } => {
                let mut st = self.state.borrow_mut();
                let Some(mut post) = st.post(&post_id) else {
                    // The post itself arrives over the ordinary `posted`
                    // event; a stream frame that beats it has nothing to
                    // write into yet, and the next frame will.
                    return;
                };
                post.message = message;
                st.apply_post(post);
                drop(st);
                // The row keeps its place in the feed; only what it says
                // changes, so the reader is not moved by an answer growing.
                self.refresh_messages(cx);
            }
            // The final text already arrived as a Text frame, and the post is
            // updated server-side too; nothing left to do.
            StreamUpdate::Done { .. } | StreamUpdate::Ignored => {}
        }
    }

    /// Asks the Agents plugin what bots exist. A server without the plugin
    /// 404s, which is indistinguishable from having no bots.
    pub(crate) fn load_bots(self: &Rc<Self>, _cx: &mut App) {
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move { crate::agents::bots(&client).await },
            move |result, cx| match result {
                Ok(bots) => {
                    ui.state.borrow_mut().bots = bots.bots;
                    ui.refresh_agent_actions(cx);
                }
                Err(e) => tracing::debug!(error = %e, "no agents plugin on this server"),
            },
        );
    }

    /// Shows or hides the agent entries, which only make sense when there is
    /// a bot to answer them.
    pub(crate) fn refresh_agent_actions(self: &Rc<Self>, cx: &mut App) {
        let bots: Vec<(String, String)> = self
            .state
            .borrow()
            .bots
            .iter()
            .map(|bot| {
                let name = if bot.display_name.is_empty() {
                    format!("@{}", bot.username)
                } else {
                    bot.display_name.clone()
                };
                // The DM channel may not exist yet; the bot's user id is
                // enough to make one.
                let target = if bot.dm_channel_id.is_empty() {
                    format!("user:{}", bot.id)
                } else {
                    format!("channel:{}", bot.dm_channel_id)
                };
                (target, format!("Chat with {name}"))
            })
            .collect();
        self.chat.set_agents(&bots, cx);
    }

    /// "Catch me up": asks the default bot to summarise what you have not read
    /// in this channel. The answer is written into a DM post, so this only
    /// starts it — the text arrives over the socket.
    pub(crate) fn summarise_unreads(self: &Rc<Self>, cx: &mut App) {
        let (client, channel) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_channel.clone())
        };
        let Some(channel_id) = channel else { return };

        let ui = self.clone();
        self.toast("Asking the agent…", cx);
        runtime::spawn(
            async move { crate::agents::summarise_unreads(&client, &channel_id).await },
            move |result, cx| match result {
                // The summary is a DM from the bot, so go and read it there.
                Ok(target) => ui.dispatch(Action::OpenPost(target.channel_id, target.post_id), cx),
                Err(e) => ui.toast(&format!("The agent could not answer: {e}"), cx),
            },
        );
    }
}
