use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::Post;

use super::ui::Ui;
use crate::runtime;
use crate::timefmt::{format_day, format_time};
use crate::ui::dialogs;

impl Ui {
    /// Shows what a message used to say, newest first, each with the time it
    /// was replaced.
    pub(crate) fn show_history(self: &Rc<Self>, versions: Vec<Post>, cx: &mut App) {
        // The current text is the first entry the server returns, so anything
        // after it is something you could go back to.
        let rows = versions
            .into_iter()
            .enumerate()
            .map(|(index, version)| {
                let mut row = dialogs::Row::new(
                    version.id.clone(),
                    format!(
                        "{} {}",
                        format_day(version.create_at),
                        format_time(version.create_at)
                    ),
                )
                .body(version.source_text());
                if index > 0 {
                    let ui = self.clone();
                    let post_id = version.original_id.clone();
                    let version_id = version.id.clone();
                    row = row.button("Restore this version", "Restoring…", move |_cx| {
                        let client = ui.state.borrow().client.clone();
                        let post_id = post_id.clone();
                        let version_id = version_id.clone();
                        let ui = ui.clone();
                        runtime::spawn(
                            async move { client.restore_post_version(&post_id, &version_id).await },
                            move |result, cx| match result {
                                Ok(_) => ui.toast("Restored.", cx),
                                Err(e) => ui.toast(&format!("Could not restore it: {e}"), cx),
                            },
                        );
                    });
                }
                row
            })
            .collect();
        dialogs::show_rows(self, cx, "Edit history", rows, None);
    }

    /// Applies an edit. An emptied message means delete, which is what the
    /// other clients do and what the server expects.
    pub(crate) fn submit_edit(self: &Rc<Self>, post_id: String, text: String, cx: &mut App) {
        self.chat.end_edit(cx);
        if text.trim().is_empty() {
            self.confirm_delete(post_id, cx);
            return;
        }
        let client = self.state.borrow().client.clone();
        let ui = self.clone();
        runtime::spawn(
            async move { client.update_post(&post_id, &text).await },
            move |result, cx| {
                // Success arrives as post_edited over the socket, so only the
                // failure needs saying.
                if let Err(e) = result {
                    ui.toast(&format!("Could not save the edit: {e}"), cx);
                }
            },
        );
    }
}
