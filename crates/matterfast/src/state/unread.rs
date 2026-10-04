use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    pub fn unread(&self, channel_id: &str) -> UnreadState {
        match (
            self.channels.get(channel_id),
            self.memberships.get(channel_id),
        ) {
            (Some(c), Some(m)) => UnreadState::compute(c, m, self.crt_enabled),
            _ => UnreadState::default(),
        }
    }

    /// Unread thread count for the badge on the inbox button.
    pub fn unread_threads(&self) -> i64 {
        self.thread_inbox.iter().filter(|t| t.is_unread()).count() as i64
    }

    /// Total mentions across the current team — what a tray badge would show.
    pub fn total_mentions(&self) -> i64 {
        self.channels
            .keys()
            .map(|id| self.unread(id))
            .map(|u| u.mentions)
            .sum()
    }
}
