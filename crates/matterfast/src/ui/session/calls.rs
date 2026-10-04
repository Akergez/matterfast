//! Small questions about a call's roster and jobs.

use mattermost_calls::protocol::{CallState, JobState};

/// The distinct people in a call. A roster is keyed by *session*, and one
/// person joining from two devices holds two of them.
pub(crate) fn participants(state: &CallState) -> Vec<String> {
    let mut people: Vec<String> = Vec::with_capacity(state.sessions.len());
    for session in &state.sessions {
        if !people.contains(&session.user_id) {
            people.push(session.user_id.clone());
        }
    }
    people
}

/// Whether a recording/transcription job is actually running right now.
pub(crate) fn is_running(job: Option<&JobState>) -> bool {
    job.is_some_and(|j| j.start_at > 0 && j.end_at == 0 && j.err.is_empty())
}
