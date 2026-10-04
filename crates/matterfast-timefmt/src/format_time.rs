use mattermost_api::models::Millis;

use crate::local::local;

pub fn format_time(millis: Millis) -> String {
    local(millis)
        .map(|at| at.format("%H:%M").to_string())
        .unwrap_or_default()
}
