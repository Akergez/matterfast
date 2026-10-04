use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    pub fn apply_statuses(&mut self, statuses: Vec<Status>) {
        for status in statuses {
            // The last-activity stamp comes along with presence and is the
            // only thing that can answer "when were they last here".
            if status.last_activity_at > 0 {
                self.last_seen
                    .insert(status.user_id.clone(), status.last_activity_at);
            }
            self.statuses
                .insert(status.user_id.clone(), status.presence());
        }
    }

    pub fn presence(&self, user_id: &str) -> Presence {
        self.statuses.get(user_id).copied().unwrap_or_default()
    }
}
