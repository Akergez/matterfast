use chrono::{DateTime, Local};

/// Reads the timestamp a custom status carries.
pub(crate) fn parse_expiry(expires_at: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(expires_at)
        .ok()
        .map(|at| at.with_timezone(&Local))
}
