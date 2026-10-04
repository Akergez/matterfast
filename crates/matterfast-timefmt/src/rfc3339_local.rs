use mattermost_api::models::Millis;

use crate::local::local;

/// A moment as RFC 3339 in the local zone, which is what the custom-status
/// route wants — unlike every other time in this API. It is decoded into a Go
/// `time.Time`, which insists on the colon in the zone offset: "+0300" fails
/// to parse and comes back as "invalid or missing custom_status".
pub fn rfc3339_local(millis: Millis) -> Option<String> {
    local(millis).map(|at| at.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;

    #[test]
    fn the_status_route_gets_a_colon_in_its_zone_offset() {
        let stamp = rfc3339_local(1_700_000_000_000).unwrap();
        let zone = &stamp[stamp.len() - 6..];
        assert!(
            zone.starts_with(['+', '-']) && zone.as_bytes()[3] == b':',
            "got {stamp:?}"
        );
        // And it reads back as the same moment.
        assert_eq!(
            DateTime::parse_from_rfc3339(&stamp)
                .unwrap()
                .timestamp_millis(),
            1_700_000_000_000
        );
    }
}
