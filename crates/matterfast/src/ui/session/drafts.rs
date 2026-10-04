//! Drafts for channels and threads, kept locally and synced when the server
//! allows it.

use std::rc::Rc;

use gpui_kit::App;

use super::ui::Ui;
use crate::runtime;
use crate::ui::rhs::PanelMode;

const SAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(900);

impl Ui {
    /// A thread's reply box has its own draft, keyed by the thread root —
    /// which is how the server stores them too, so they sync with the other
    /// clients rather than only surviving locally.
    pub(crate) fn schedule_thread_draft_save(self: &Rc<Self>, _cx: &mut App) {
        if self.thread_draft_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        runtime::after(SAVE_DEBOUNCE, move |cx| {
            ui.thread_draft_pending.set(false);
            ui.save_thread_draft(cx);
        });
    }

    pub(crate) fn save_thread_draft(self: &Rc<Self>, cx: &mut App) {
        let PanelMode::Thread(root_id) = self.right.mode(cx) else {
            return;
        };
        let text = self.right.composer_text(cx);
        let (client, channel_id, synced, changed) = {
            let mut st = self.state.borrow_mut();
            let Some(channel_id) = st.find_post(&root_id).map(|p| p.channel_id.clone()) else {
                return;
            };
            let trimmed = text.trim().to_string();
            let changed = if trimmed.is_empty() {
                st.thread_drafts.remove(&root_id).is_some()
            } else {
                st.thread_drafts.insert(root_id.clone(), trimmed.clone()) != Some(trimmed)
            };
            (st.client.clone(), channel_id, st.drafts_synced, changed)
        };
        if !changed || !synced {
            return;
        }

        let body = text.trim().to_string();
        let draft = mattermost_api::models::Draft::new(&channel_id, &root_id, &body);
        runtime::spawn(
            async move {
                if body.is_empty() {
                    client.delete_draft(&channel_id, &root_id).await
                } else {
                    client.upsert_draft(&draft).await.map(|_| ())
                }
            },
            move |result, _cx| {
                if let Err(e) = result {
                    tracing::warn!(error = %e, "could not save the thread draft");
                }
            },
        );
    }

    pub(crate) fn restore_thread_draft(&self, cx: &mut App) {
        let text = match self.right.mode(cx) {
            PanelMode::Thread(root_id) => self
                .state
                .borrow()
                .thread_drafts
                .get(&root_id)
                .cloned()
                .unwrap_or_default(),
            _ => return,
        };
        self.right.set_composer_text(&text, cx);
    }

    /// Saves the composer as a draft shortly after typing stops.
    ///
    /// Debounced rather than per-keystroke: a draft is worth one request when
    /// someone pauses, not one per character.
    pub(crate) fn schedule_draft_save(self: &Rc<Self>, _cx: &mut App) {
        if self.draft_save_pending.replace(true) {
            return;
        }
        let ui = self.clone();
        runtime::after(SAVE_DEBOUNCE, move |cx| {
            ui.draft_save_pending.set(false);
            ui.save_draft(cx);
        });
    }

    /// Stores the current composer text for the channel it belongs to, locally
    /// and — when the server keeps drafts — there too.
    pub(crate) fn save_draft(self: &Rc<Self>, cx: &mut App) {
        let text = self.chat.composer_text(cx);
        let (client, channel_id, synced, changed) = {
            let mut st = self.state.borrow_mut();
            let Some(channel_id) = st.current_channel.clone() else {
                return;
            };
            let trimmed = text.trim().to_string();
            let changed = if trimmed.is_empty() {
                st.drafts.remove(&channel_id).is_some()
            } else {
                st.drafts.insert(channel_id.clone(), trimmed.clone()) != Some(trimmed)
            };
            (st.client.clone(), channel_id, st.drafts_synced, changed)
        };
        if !changed {
            return;
        }
        // The pencil in the sidebar has to follow.
        self.channels.refresh(cx);
        if !synced {
            return;
        }

        let ui = self.clone();
        let body = text.trim().to_string();
        let draft = mattermost_api::models::Draft::new(&channel_id, "", &body);
        runtime::spawn(
            async move {
                // An empty upsert deletes server-side, but the explicit route
                // says what is meant and does not depend on that behaviour
                // staying true.
                if body.is_empty() {
                    client.delete_draft(&channel_id, "").await
                } else {
                    client.upsert_draft(&draft).await.map(|_| ())
                }
            },
            move |result, _cx| {
                if let Err(e) = result {
                    // 501 means the admin turned synced drafts off. Stop
                    // asking; the local copy still works.
                    if matches!(&e, mattermost_api::Error::Api(a) if a.status_code == 501) {
                        tracing::info!("synced drafts are disabled on this server");
                        ui.state.borrow_mut().drafts_synced = false;
                    } else {
                        tracing::warn!(error = %e, "could not save the draft");
                    }
                }
            },
        );
    }

    /// Pulls this team's drafts and puts the current channel's back.
    pub(crate) fn load_drafts(self: &Rc<Self>, _cx: &mut App) {
        let (client, team) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        let ui = self.clone();
        runtime::spawn(
            async move { client.my_drafts(&team_id).await },
            move |result, cx| match result {
                Ok(drafts) => {
                    {
                        let mut st = ui.state.borrow_mut();
                        for draft in drafts {
                            if draft.message.is_empty() {
                                continue;
                            }
                            if draft.root_id.is_empty() {
                                st.drafts.insert(draft.channel_id, draft.message);
                            } else {
                                st.thread_drafts.insert(draft.root_id, draft.message);
                            }
                        }
                    }
                    ui.restore_draft(cx);
                    ui.channels.refresh(cx);
                }
                Err(e) => {
                    if matches!(&e, mattermost_api::Error::Api(a) if a.status_code == 501) {
                        ui.state.borrow_mut().drafts_synced = false;
                    }
                }
            },
        );
    }

    /// Puts the current channel's draft into the composer.
    pub(crate) fn restore_draft(&self, cx: &mut App) {
        let text = {
            let st = self.state.borrow();
            st.current_channel
                .as_ref()
                .and_then(|id| st.drafts.get(id))
                .cloned()
                .unwrap_or_default()
        };
        self.chat.set_composer_text(&text, cx);
    }
}
