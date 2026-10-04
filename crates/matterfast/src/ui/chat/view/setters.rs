use gpui_kit::App;

use super::chat_view::ChatView;
use crate::ui::autocomplete::Candidate;

impl ChatView {
    /// Marks the first page of a channel as in flight. Only matters while
    /// there is nothing to show: a reload over existing messages should leave
    /// them on screen rather than blank the pane.
    pub fn set_loading(&self, loading: bool, cx: &mut App) {
        self.loading.set(loading);
        cx.refresh_windows();
    }

    pub fn set_typing(&self, names: &[String], cx: &mut App) {
        if *self.typing.borrow() != names {
            *self.typing.borrow_mut() = names.to_vec();
            cx.refresh_windows();
        }
    }

    /// `None` means connected. Anything else is shown until it is cleared.
    pub fn set_connection_problem(&self, problem: Option<&str>, cx: &mut App) {
        *self.connection.borrow_mut() = problem.map(str::to_string);
        cx.refresh_windows();
    }

    /// How many people are in the channel, shown beside the topic.
    pub fn set_member_count(&self, count: Option<i64>, cx: &mut App) {
        self.member_count.set(count);
        cx.refresh_windows();
    }

    /// Rebuilds the agent menu: one entry to summarise this channel, and one
    /// per bot to go and talk to it. `bots` is (target, label).
    pub fn set_agents(&self, bots: &[(String, String)], cx: &mut App) {
        *self.agents.borrow_mut() = bots.to_vec();
        cx.refresh_windows();
    }

    /// A file being uploaded shows as a chip that is not yet removable.
    pub fn set_uploading(&self, count: usize, cx: &mut App) {
        self.uploading.set(self.uploading.get() + count);
        cx.refresh_windows();
    }

    /// One of the uploads in flight has landed, or failed.
    pub fn upload_finished(&self, cx: &mut App) {
        self.uploading.set(self.uploading.get().saturating_sub(1));
        cx.refresh_windows();
    }

    /// Answers an outstanding completion query.
    pub fn set_completions(&self, items: Vec<Candidate>, cx: &mut App) {
        self.completions.borrow_mut().set(items);
        cx.refresh_windows();
    }

    pub fn set_calls_available(&self, available: bool, reason: Option<&str>, cx: &mut App) {
        self.calls_available.set(available);
        *self.calls_reason.borrow_mut() = reason.map(str::to_string);
        cx.refresh_windows();
    }

    /// Reflects our own membership. Everything you can *do* inside a call is
    /// in the dock; the header only starts, joins or ends one.
    pub fn set_in_call(&self, in_call: bool, ongoing: bool, cx: &mut App) {
        self.in_call.set(in_call);
        self.call_ongoing.set(ongoing);
        cx.refresh_windows();
    }

    pub fn set_call_in_progress(&self, participants: Option<usize>, cx: &mut App) {
        self.call_participants.set(participants);
        cx.refresh_windows();
    }

    /// Says, at the top of the feed, that the page before this one is on its
    /// way. Without it a scrollback that takes a moment looks like the start
    /// of the channel.
    pub fn set_loading_older(&self, loading: bool, cx: &mut App) {
        self.loading_older.set(loading);
        cx.refresh_windows();
    }

    /// A network error should be retryable on the next scroll even if the
    /// reader is still close to the history edge.
    pub fn retry_older_on_next_edge_change(&self, _cx: &App) {
        self.pagination_armed.set(true);
    }
}
