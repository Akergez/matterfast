use crate::state::{ActiveCall, AppState};

/// The dock's two lines: who is talking, and where.
///
/// Server-side voice activity only ever names other people, so a quiet call
/// falls back to the channel — never to "you are talking". The first value is
/// the speaker's id and name, when there is one.
pub(super) fn summary(
    call: &ActiveCall,
    st: &AppState,
) -> (Option<(String, String)>, String, String) {
    let speaker = call
        .speaking
        .first()
        .and_then(|id| st.users.get(id))
        .map(|user| {
            (
                user.id.clone(),
                user.display_name(st.teammate_name_display()),
            )
        });
    let title = match &speaker {
        Some((_, name)) => format!("{name} is talking"),
        None => "In a call".to_string(),
    };
    let channel = st
        .channel(&call.channel_id)
        .map(|c| st.channel_title(c))
        .unwrap_or_else(|| "a channel".to_string());
    let people = st.active_calls.get(&call.channel_id).map_or(1, Vec::len);
    let sharing = if call.sharing.is_empty() {
        ""
    } else {
        " · sharing"
    };
    (
        speaker,
        title,
        format!("{channel} · {people} in the call{sharing}"),
    )
}
