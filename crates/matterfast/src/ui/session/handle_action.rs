//! The one place an [`Action`] is turned into what it does.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::rhs::PanelMode;
use crate::ui::Action;

impl Ui {
    pub(crate) fn handle(self: &Rc<Self>, action: Action, cx: &mut App) {
        match action {
            Action::SelectTeam(team_id) => self.select_team(team_id, cx),
            Action::SelectChannel(channel_id) => self.select_channel(channel_id, cx),
            Action::LoadChannel(channel_id) => self.load_channel(channel_id, cx),
            Action::Send(text) => {
                // The composer is shared with editing, so what "send" means
                // depends on which mode it is in.
                match self.chat.editing(cx) {
                    Some(post_id) => self.submit_edit(post_id, text, cx),
                    None => self.send_message(text, None, cx),
                }
                // The composer is empty now, so the draft has to go with it —
                // and immediately, not on the debounce.
                self.save_draft(cx);
            }
            Action::ToggleCall => self.toggle_call(cx),
            Action::ToggleMute => self.toggle_mute(cx),
            Action::ToggleRecording => self.toggle_recording(cx),
            Action::ToggleScreen => self.toggle_screen(cx),
            Action::ToggleCamera => self.toggle_camera(cx),
            Action::OpenThread(root_id) => self.open_thread(root_id, cx),
            Action::ReplyInThread(text) => {
                // The reply box is empty after this, and the draft has to go
                // with it — immediately, not on the debounce.
                let flush = self.clone();
                runtime::soon(move |cx| flush.save_thread_draft(cx));
                if let PanelMode::Thread(root) = self.right.mode(cx) {
                    self.send_message(text, Some(root), cx);
                }
            }
            Action::OpenInbox => self.open_inbox(cx),
            Action::OlderThreads => self.load_older_threads(cx),
            Action::CloseRightPanel => {
                self.right.set_mode(PanelMode::Hidden, cx);
                self.overlay.set_show_sidebar(false, cx);
            }
            Action::ToggleReaction(post_id, emoji) => self.toggle_reaction(post_id, emoji, cx),
            Action::CardAction {
                post_id,
                action_id,
                selected,
                cookie,
            } => self.card_action(post_id, action_id, selected, cookie, cx),
            Action::OpenPost(channel_id, root_id) => {
                self.select_channel(channel_id, cx);
                if !root_id.is_empty() {
                    self.open_thread(root_id, cx);
                }
            }
            Action::JumpToPost(channel_id, post_id) => self.jump_to_post(channel_id, post_id, cx),
            Action::OpenDirectMessage(user_id) => self.open_direct_message(user_id, cx),
            Action::Post(post_id, what) => self.post_action(post_id, what, cx),
            Action::Search(terms) => self.search(terms, cx),
            Action::SearchMore => self.search_more(cx),
            Action::SearchPeople => self.search_people(),
            Action::Complete(query) => self.complete(query, cx),
            Action::ScheduleMessage => self.schedule_message(cx),
            Action::ThreadDraftChanged => self.schedule_thread_draft_save(cx),
            Action::LoadOlder => self.load_older(cx),
            Action::FollowThread(following) => self.follow_thread(following, cx),
            Action::PickAttachment => self.pick_attachment(cx),
            Action::AttachFiles(paths) => {
                let Some(channel_id) = self.state.borrow().current_channel.clone() else {
                    return;
                };
                self.chat.set_uploading(paths.len(), cx);
                for path in paths {
                    self.upload(&channel_id, path, cx);
                }
            }
            Action::SummariseUnreads => self.summarise_unreads(cx),
            Action::SetStatus(status) => self.set_status(status, cx),
            Action::Row(channel_id, what) => self.row_action(channel_id, what, cx),
            Action::ToggleHand => self.toggle_hand(cx),
            Action::HostControl(session_id, what) => self.host_control(session_id, what, cx),
            Action::CallReaction(name, glyph) => {
                let Some(session) = self.state.borrow().call.as_ref().map(|c| c.session.clone())
                else {
                    return;
                };
                let ui = self.clone();
                runtime::spawn(
                    async move {
                        session
                            .react(&mattermost_calls::CallReaction {
                                name: name.clone(),
                                literal: glyph,
                                ..Default::default()
                            })
                            .await
                    },
                    move |result, cx| {
                        if let Err(e) = result {
                            ui.toast(&format!("Could not react: {e}"), cx);
                        }
                    },
                );
            }
            Action::DropAttachment(file_id) => {
                self.state
                    .borrow_mut()
                    .pending_files
                    .retain(|(id, _)| id != &file_id);
                self.refresh_attachments(cx);
            }
            Action::ComposerChanged(has_text) => {
                if has_text {
                    self.notify_typing(cx);
                }
                // Emptying the composer is a draft change too — that is how a
                // draft gets deleted.
                self.schedule_draft_save(cx);
            }
            Action::OpenCallChannel => {
                let channel = self
                    .state
                    .borrow()
                    .call
                    .as_ref()
                    .map(|c| c.channel_id.clone());
                if let Some(id) = channel {
                    self.select_channel(id, cx);
                }
            }
        }
    }
}
