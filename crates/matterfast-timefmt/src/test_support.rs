use chrono::Local;

/// An RFC 3339 stamp this many hours from now.
pub(crate) fn in_hours(hours: i64) -> String {
    (Local::now() + chrono::Duration::hours(hours))
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}
