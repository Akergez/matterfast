//! Messages sent later: the server keeps them and posts them for you.

use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::models::{Post, ScheduledPost, TeamScheduledPosts};

use super::ui::Ui;
use crate::runtime;
use crate::timefmt::{format_day, format_time};
use crate::ui::kit::Lucide;
use crate::ui::dialogs;

impl Ui {
    /// Sends what is in the composer at a chosen time.
    ///
    /// The server keeps it and posts it for you, so this works with the app
    /// closed — which is the only reason to use it over waiting.
    pub(crate) fn schedule_message(self: &Rc<Self>, cx: &mut App) {
        let text = self.chat.composer_text(cx);
        if text.trim().is_empty() {
            self.toast("Write the message first.", cx);
            return;
        }
        let (client, channel_id) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_channel.clone())
        };
        let Some(channel_id) = channel_id else { return };

        let ui = self.clone();
        dialogs::schedule_message(self, cx, move |when_ms, _cx| {
            let scheduled = ScheduledPost {
                post: Post {
                    channel_id: channel_id.clone(),
                    message: text.clone(),
                    ..Default::default()
                },
                scheduled_at: when_ms,
                error_code: String::new(),
            };
            let client = client.clone();
            let ui = ui.clone();
            runtime::spawn(
                async move { client.create_scheduled_post(&scheduled).await },
                move |result, cx| match result {
                    Ok(_) => {
                        // The composer is empty now, so the draft goes too.
                        ui.chat.set_composer_text("", cx);
                        ui.save_draft(cx);
                        ui.toast("Scheduled.", cx);
                    }
                    Err(e) => ui.toast(&format!("Could not schedule it: {e}"), cx),
                },
            );
        });
    }

    /// Messages waiting to be sent later, with a way to call them off.
    ///
    /// The response is a bucket map keyed by team, plus a separate one for
    /// direct messages, so this walks the values rather than assuming a shape.
    pub(crate) fn scheduled_posts(self: &Rc<Self>, _cx: &mut App) {
        let (client, team) = {
            let st = self.state.borrow();
            (st.client.clone(), st.current_team.clone())
        };
        let Some(team_id) = team else { return };

        let ui = self.clone();
        runtime::spawn(
            async move { client.scheduled_posts_for_team(&team_id).await },
            move |result, cx| match result {
                Ok(value) => ui.show_scheduled(value, cx),
                Err(e) => ui.toast(&format!("Could not load them: {e}"), cx),
            },
        );
    }

    pub(crate) fn show_scheduled(self: &Rc<Self>, scheduled: TeamScheduledPosts, cx: &mut App) {
        // Every bucket, team and direct alike: they are all messages this
        // person has waiting, and separating them here would be a
        // distinction without a difference.
        let mut posts: Vec<ScheduledPost> = scheduled.0.into_values().flatten().collect();
        posts.sort_by_key(|p| p.scheduled_at);

        let rows = posts
            .into_iter()
            .map(|scheduled| {
                // Rescheduling rather than cancel-and-retype: the message is
                // already written, and the usual reason to touch one of these
                // is that the time was wrong.
                let reschedule_ui = self.clone();
                let reschedule = scheduled.clone();
                let cancel_ui = self.clone();
                let id = scheduled.post.id.clone();
                dialogs::Row::new(
                    scheduled.post.id.clone(),
                    format!(
                        "{} {}",
                        format_day(scheduled.scheduled_at),
                        format_time(scheduled.scheduled_at)
                    ),
                )
                .body(scheduled.post.message.clone())
                .icon_button(Lucide::AlarmClock, "Change the time", move |cx| {
                    let ui = reschedule_ui.clone();
                    let scheduled = reschedule.clone();
                    dialogs::schedule_message(&reschedule_ui, cx, move |when_ms, _cx| {
                        let client = ui.state.borrow().client.clone();
                        let mut updated = scheduled.clone();
                        updated.scheduled_at = when_ms;
                        let id = updated.post.id.clone();
                        let ui = ui.clone();
                        runtime::spawn(
                            async move { client.update_scheduled_post(&id, &updated).await },
                            move |result, cx| match result {
                                Ok(_) => ui.toast("Rescheduled.", cx),
                                Err(e) => ui.toast(&format!("Could not reschedule it: {e}"), cx),
                            },
                        );
                    });
                })
                .icon_button(Lucide::Trash, "Cancel", move |_cx| {
                    let client = cancel_ui.state.borrow().client.clone();
                    let id = id.clone();
                    let ui = cancel_ui.clone();
                    runtime::spawn(
                        async move { client.delete_scheduled_post(&id).await },
                        move |result, cx| match result {
                            Ok(()) => ui.toast("Cancelled.", cx),
                            Err(e) => ui.toast(&format!("Could not cancel it: {e}"), cx),
                        },
                    );
                })
            })
            .collect();
        dialogs::show_rows(
            self,
            cx,
            "Scheduled messages",
            rows,
            Some((
                Lucide::AlarmClock,
                "Nothing scheduled",
                "Messages you send later wait here.",
            )),
        );
    }
}
