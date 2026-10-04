use std::time::{Duration, Instant};

use super::app_state::AppState;

impl AppState {
    /// Mattermost repeats `user_typing` about every five seconds while someone
    /// keeps typing, and never says they stopped, so this is the window an
    /// entry stays live for.
    pub const TYPING_TTL: Duration = Duration::from_secs(6);

    pub fn typing_started(&mut self, channel_id: String, user_id: String) {
        self.typing
            .entry(channel_id)
            .or_default()
            .insert(user_id, Instant::now());
    }

    /// Who is currently typing in a channel, oldest first, dropping anyone
    /// whose last keystroke has aged out.
    pub fn typing_in(&mut self, channel_id: &str) -> Vec<String> {
        let Some(people) = self.typing.get_mut(channel_id) else {
            return Vec::new();
        };
        people.retain(|_, at| at.elapsed() < Self::TYPING_TTL);
        let mut live: Vec<(&String, &Instant)> = people.iter().collect();
        live.sort_by_key(|(_, at)| **at);
        live.into_iter().map(|(id, _)| id.clone()).collect()
    }
}
