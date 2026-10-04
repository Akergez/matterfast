use mattermost_api::models::CustomStatus;

use crate::timefmt::has_expired;

/// The status as a person would read it: what it says, and until when.
///
/// The expiry is most of the information — "on holiday" matters differently
/// depending on whether they are back this afternoon or next week — and it is
/// what the other clients show alongside it.
pub fn custom_status_tooltip(status: &CustomStatus) -> String {
    let text = if status.text.is_empty() {
        format!(":{}:", status.emoji)
    } else {
        status.text.clone()
    };
    match status
        .expires_at
        .as_deref()
        .and_then(crate::timefmt::expiry_phrase)
    {
        Some(until) => format!("{text}\n{until}"),
        None => text,
    }
}

/// Whether a custom status is worth drawing: it says something, and it has
/// not run out.
pub fn status_is_live(status: &CustomStatus) -> bool {
    if status.emoji.is_empty() && status.text.is_empty() {
        return false;
    }
    !status
        .expires_at
        .as_deref()
        .is_some_and(|expiry| !expiry.is_empty() && has_expired(expiry))
}
