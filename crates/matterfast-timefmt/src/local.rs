use chrono::{DateTime, Local, TimeZone};
use mattermost_api::models::Millis;

pub(crate) fn local(millis: Millis) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(millis).single()
}
