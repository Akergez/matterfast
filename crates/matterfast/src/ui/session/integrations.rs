//! Integration dialogs, card buttons and slash commands.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::interactive;

impl Ui {
    /// Shows an integration's form and posts the answers back.
    ///
    /// The server does not interpret the submission — it forwards it to the
    /// integration's own URL, which is why that URL has to be echoed back
    /// exactly as it arrived.
    pub(crate) fn open_dialog(
        self: &Rc<Self>,
        request: mattermost_api::models::dialog::OpenDialogRequest,
        cx: &mut App,
    ) {
        let (client, me, channel_id, team_id) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.me.id.clone(),
                st.current_channel.clone().unwrap_or_default(),
                st.current_team.clone().unwrap_or_default(),
            )
        };

        let dialog = request.dialog.clone();
        let submit_ctx = (
            client.clone(),
            request.url.clone(),
            dialog.callback_id.clone(),
            dialog.state.clone(),
            me.clone(),
            channel_id.clone(),
            team_id.clone(),
        );
        let cancel_ctx = submit_ctx.clone();
        let notify_on_cancel = dialog.notify_on_cancel;
        let ui = self.clone();

        interactive::present(
            self,
            cx,
            &dialog,
            move |submission, _cx| {
                let (client, url, callback_id, state, user_id, channel_id, team_id) =
                    submit_ctx.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move {
                        let request = mattermost_api::models::dialog::SubmitDialogRequest {
                            url,
                            callback_id,
                            state,
                            user_id,
                            channel_id,
                            team_id,
                            submission,
                            cancelled: false,
                        };
                        client.submit_dialog(&request).await
                    },
                    move |result, cx| match result {
                        // Errors come back inside a 200 as well, since the
                        // integration is what validated the form.
                        Ok(response) => {
                            if let Some(error) = response
                                .get("error")
                                .and_then(|v| v.as_str())
                                .filter(|e| !e.is_empty())
                            {
                                ui.toast(error, cx);
                            }
                        }
                        Err(e) => ui.toast(&format!("That form was not accepted: {e}"), cx),
                    },
                );
            },
            move |_cx| {
                // Only when the dialog asked to be told; most do not care.
                if !notify_on_cancel {
                    return;
                }
                let (client, url, callback_id, state, user_id, channel_id, team_id) =
                    cancel_ctx.clone();
                runtime::spawn(
                    async move {
                        let request = mattermost_api::models::dialog::SubmitDialogRequest {
                            url,
                            callback_id,
                            state,
                            user_id,
                            channel_id,
                            team_id,
                            submission: Default::default(),
                            cancelled: true,
                        };
                        client.submit_dialog(&request).await
                    },
                    |_, _| {},
                );
            },
        );
    }

    /// Presses something on a card. What it does is the integration's
    /// business and comes back over the websocket — the post edited in place,
    /// an ephemeral message, a dialog — so only a failure is ours to report.
    pub(crate) fn card_action(
        self: &Rc<Self>,
        post_id: String,
        action_id: String,
        selected: String,
        cookie: String,
        _cx: &mut App,
    ) {
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move {
                client
                    .do_post_action(&post_id, &action_id, &selected, &cookie)
                    .await
            },
            move |result, cx| {
                if let Err(e) = result {
                    ui.toast(&format!("That did not go through: {e}"), cx);
                }
            },
        );
    }

    /// Runs a slash command. Its output arrives as a post or an ephemeral
    /// message, so there is usually nothing to show from the response itself.
    pub(crate) fn run_command(self: &Rc<Self>, channel_id: String, command: String, cx: &mut App) {
        let client = self.state.borrow().client.clone();
        self.chat.set_composer_text("", cx);
        let ui = self.clone();
        runtime::spawn(
            async move { client.execute_command(&channel_id, &command).await },
            move |result, cx| match result {
                Ok(response) => {
                    // Some commands answer with somewhere to go rather than
                    // something to say.
                    if let Some(location) = response
                        .get("goto_location")
                        .and_then(|v| v.as_str())
                        .filter(|l| !l.is_empty())
                    {
                        crate::open_url(location, cx);
                    }
                }
                Err(e) => ui.toast(&format!("That command failed: {e}"), cx),
            },
        );
    }
}
