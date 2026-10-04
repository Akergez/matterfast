//! What a message's own menu can ask for.

use std::rc::Rc;

use gpui_kit::{App, ClipboardItem};

use super::ui::Ui;
use crate::runtime;
use crate::ui::message::PostAction;
use crate::ui::rhs::PanelMode;
use crate::ui::{dialogs, Action};

impl Ui {
    /// Everything a message's own menu can ask for.
    pub(crate) fn post_action(self: &Rc<Self>, post_id: String, what: PostAction, cx: &mut App) {
        let (client, me, post) = {
            let st = self.state.borrow();
            (st.client.clone(), st.me.id.clone(), st.post(&post_id))
        };
        let Some(post) = post else {
            self.toast("That message is no longer here.", cx);
            return;
        };

        match what {
            PostAction::CopyText => {
                cx.write_to_clipboard(ClipboardItem::new_string(post.message.clone()));
                self.toast("Message copied.", cx);
            }
            PostAction::CopyLink => {
                let link = self.permalink(client.site_url(), &post_id);
                cx.write_to_clipboard(ClipboardItem::new_string(link));
                self.toast("Link copied.", cx);
            }
            PostAction::Summarise => {
                if self.state.borrow().bots.is_empty() {
                    self.toast("This server has no agent to ask.", cx);
                    return;
                }
                let ui = self.clone();
                self.toast("Asking the agent…", cx);
                runtime::spawn(
                    async move { crate::agents::summarise_thread(&client, &post_id).await },
                    move |result, cx| match result {
                        Ok(target) => {
                            ui.dispatch(Action::OpenPost(target.channel_id, target.post_id), cx)
                        }
                        Err(e) => ui.toast(&format!("The agent could not answer: {e}"), cx),
                    },
                );
            }
            PostAction::Remind => {
                let ui = self.clone();
                let me = me.clone();
                dialogs::post_reminder(self, cx, move |when_ms, _cx| {
                    let client = client.clone();
                    let me = me.clone();
                    let post_id = post_id.clone();
                    let ui = ui.clone();
                    runtime::spawn(
                        // The reminder route takes seconds, unlike everything
                        // else in this API.
                        async move {
                            client
                                .set_post_reminder(&me, &post_id, when_ms / 1000)
                                .await
                        },
                        move |result, cx| match result {
                            Ok(()) => ui.toast("You will be reminded.", cx),
                            Err(e) => ui.toast(&format!("Could not set that: {e}"), cx),
                        },
                    );
                });
            }
            PostAction::History => {
                let ui = self.clone();
                runtime::spawn(
                    async move { client.post_edit_history(&post_id).await },
                    move |result, cx| match result {
                        Ok(versions) if versions.is_empty() => {
                            ui.toast("No earlier versions are kept for this message.", cx)
                        }
                        Ok(versions) => ui.show_history(versions, cx),
                        Err(e) => ui.toast(&format!("Could not load the history: {e}"), cx),
                    },
                );
            }
            PostAction::Acknowledge | PostAction::Unacknowledge => {
                let ack = what == PostAction::Acknowledge;
                let ui = self.clone();
                runtime::spawn(
                    async move {
                        if ack {
                            client.acknowledge_post(&me, &post_id).await.map(|_| ())
                        } else {
                            client.unacknowledge_post(&me, &post_id).await
                        }
                    },
                    move |result, cx| {
                        // The server broadcasts the change, which is what
                        // redraws the row; only a failure needs saying.
                        if let Err(e) = result {
                            ui.toast(&format!("Could not do that: {e}"), cx);
                        }
                    },
                );
            }
            PostAction::Forward => {
                // Forwarding is a new message carrying a permalink: the server
                // resolves that back into the original, which is how the other
                // clients do it and why the quoted post stays live rather than
                // becoming a stale copy.
                let link = self.permalink(client.site_url(), &post_id);
                let ui = self.clone();
                self.pick_channel(
                    move |channel_id, _cx| {
                        let client = client.clone();
                        let link = link.clone();
                        let ui = ui.clone();
                        runtime::spawn(
                            async move { client.send_message(&channel_id, &link, None).await },
                            move |result, cx| match result {
                                Ok(post) => ui
                                    .dispatch(Action::OpenPost(post.channel_id, String::new()), cx),
                                Err(e) => ui.toast(&format!("Could not forward it: {e}"), cx),
                            },
                        );
                    },
                    cx,
                );
            }
            PostAction::MoveThread => {
                let ui = self.clone();
                self.pick_channel(
                    move |channel_id, _cx| {
                        let client = client.clone();
                        let post_id = post_id.clone();
                        let ui = ui.clone();
                        runtime::spawn(
                            async move { client.move_thread(&post_id, &channel_id).await },
                            move |result, cx| match result {
                                Ok(()) => ui.toast("Thread moved.", cx),
                                Err(e) => ui.toast(&format!("Could not move it: {e}"), cx),
                            },
                        );
                    },
                    cx,
                );
            }
            PostAction::Edit => self.chat.begin_edit(&post_id, post.source_text(), cx),
            PostAction::Delete => self.confirm_delete(post_id, cx),
            PostAction::Pin | PostAction::Unpin => {
                let pin = what == PostAction::Pin;
                let ui = self.clone();
                runtime::spawn(
                    async move { client.pin_post(&post_id, pin).await },
                    move |result, cx| match result {
                        // The server echoes the change as post_edited, so
                        // there is nothing to apply here.
                        Ok(()) => ui.toast(if pin { "Pinned." } else { "Unpinned." }, cx),
                        Err(e) => ui.toast(&format!("Could not change the pin: {e}"), cx),
                    },
                );
            }
            PostAction::Save | PostAction::Unsave => {
                let save = what == PostAction::Save;
                self.set_saved(post_id, save, cx);
            }
            PostAction::MarkUnread => {
                let (crt, channel) = {
                    let st = self.state.borrow();
                    (st.crt_enabled, post.channel_id.clone())
                };
                let ui = self.clone();
                runtime::spawn(
                    async move { client.set_post_unread(&me, &post_id, crt).await },
                    move |result, cx| match result {
                        Ok(()) => {
                            // Nothing is being read here any more, so stop
                            // marking it read on the way out.
                            if ui.state.borrow().current_channel.as_deref()
                                == Some(channel.as_str())
                            {
                                ui.state.borrow_mut().current_channel = None;
                                ui.chat.set_composer_text("", cx);
                            }
                            ui.schedule_sidebar_reload(cx);
                        }
                        Err(e) => ui.toast(&format!("Could not mark it unread: {e}"), cx),
                    },
                );
            }
        }
    }

    /// The link another client resolves back to this post.
    fn permalink(&self, site_url: &str, post_id: &str) -> String {
        let team = self
            .state
            .borrow()
            .current_team_name()
            .unwrap_or_else(|| "_redirect".to_string());
        format!("{site_url}/{team}/pl/{post_id}")
    }

    /// Deleting is destructive and has no undo, so it asks first.
    pub(super) fn confirm_delete(self: &Rc<Self>, post_id: String, cx: &mut App) {
        let ui = self.clone();
        dialogs::confirm(
            self,
            cx,
            "Delete this message?",
            "It will be removed for everyone. This cannot be undone.",
            "Delete",
            true,
            move |_cx| {
                let client = ui.state.borrow().client.clone();
                let post_id = post_id.clone();
                let ui = ui.clone();
                runtime::spawn(
                    async move { client.delete_post(&post_id).await },
                    move |result, cx| {
                        if let Err(e) = result {
                            ui.toast(&format!("Could not delete it: {e}"), cx);
                        }
                    },
                );
            },
        );
    }

    /// Saving a post is a preference, not a post field, so it is written and
    /// mirrored locally rather than waiting for an echo that never comes.
    fn set_saved(self: &Rc<Self>, post_id: String, save: bool, cx: &mut App) {
        let (client, me) = {
            let mut st = self.state.borrow_mut();
            if save {
                st.saved_posts.insert(post_id.clone());
            } else {
                st.saved_posts.remove(&post_id);
            }
            (st.client.clone(), st.me.id.clone())
        };
        self.refresh_messages(cx);

        let ui = self.clone();
        let id = post_id.clone();
        runtime::spawn(
            async move {
                let pref = mattermost_api::models::Preference {
                    user_id: me.clone(),
                    category: "flagged_post".into(),
                    name: id.clone(),
                    value: "true".into(),
                };
                if save {
                    client.save_preferences(&me, &[pref]).await
                } else {
                    client.delete_preferences(&me, &[pref]).await
                }
            },
            move |result, cx| {
                if let Err(e) = result {
                    // Put the local view back where the server still has it.
                    let mut st = ui.state.borrow_mut();
                    if save {
                        st.saved_posts.remove(&post_id);
                    } else {
                        st.saved_posts.insert(post_id.clone());
                    }
                    drop(st);
                    ui.refresh_messages(cx);
                    ui.toast(&format!("Could not change that: {e}"), cx);
                }
            },
        );
    }

    /// Follows or unfollows the open thread. Following is what keeps a thread
    /// in the inbox after you stop being mentioned in it.
    pub(crate) fn follow_thread(self: &Rc<Self>, following: bool, cx: &mut App) {
        let PanelMode::Thread(root_id) = self.right.mode(cx) else {
            return;
        };
        let (client, team_id) = {
            let st = self.state.borrow();
            (
                st.client.clone(),
                st.current_team.clone().unwrap_or_default(),
            )
        };
        let ui = self.clone();
        runtime::spawn(
            async move { client.follow_thread(&team_id, &root_id, following).await },
            move |result, cx| match result {
                Ok(()) => ui.load_inbox(cx),
                Err(e) => ui.toast(&format!("Could not change that: {e}"), cx),
            },
        );
    }
}
