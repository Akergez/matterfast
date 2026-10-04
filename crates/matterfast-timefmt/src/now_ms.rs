use chrono::Local;
use mattermost_api::models::Millis;

/// The wall clock, in the unit the server speaks.
pub fn now_ms() -> Millis {
    Local::now().timestamp_millis()
}
