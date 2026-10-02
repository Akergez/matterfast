//! Times, the way a person reads them.
//!
//! Mattermost timestamps are Unix **milliseconds**, everywhere except the few
//! places noted below, and everything shown to a person is in their own zone.

use chrono::{DateTime, Datelike, Days, Local, TimeZone};
use mattermost_api::models::Millis;

/// The wall clock, in the unit the server speaks.
pub fn now_ms() -> Millis {
    Local::now().timestamp_millis()
}

/// A number no earlier call returned. Used to name things that only have to
/// be distinct for the life of the process: a message not yet confirmed by
/// the server, a temporary file.
pub fn unique() -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};
    static LAST: AtomicI64 = AtomicI64::new(0);
    let now = now_ms();
    // The clock is the starting point, so two runs do not collide either; the
    // counter is what makes two calls in one millisecond differ.
    LAST.fetch_max(now, Ordering::Relaxed);
    LAST.fetch_add(1, Ordering::Relaxed) + 1
}

fn local(millis: Millis) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(millis).single()
}

pub fn format_time(millis: Millis) -> String {
    local(millis)
        .map(|at| at.format("%H:%M").to_string())
        .unwrap_or_default()
}

pub fn format_day(millis: Millis) -> String {
    let Some(at) = local(millis) else {
        return String::new();
    };
    let today = Local::now().date_naive();
    if at.date_naive() == today {
        return "Today".to_string();
    }
    if today.checked_sub_days(Days::new(1)) == Some(at.date_naive()) {
        return "Yesterday".to_string();
    }
    // `%e` pads a single-digit day with a space, which reads as a typo.
    at.format("%A, %-d %B %Y").to_string()
}

/// Relative time for list views ("2m", "3h", "Tue").
pub fn format_relative(millis: Millis) -> String {
    let Some(then) = local(millis) else {
        return String::new();
    };
    let seconds = (Local::now() - then).num_seconds();
    match seconds {
        s if s < 60 => "now".to_string(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s if s < 7 * 86_400 => then.format("%a").to_string(),
        _ => then.format("%-d %b").to_string(),
    }
}

/// A moment as RFC 3339 in the local zone, which is what the custom-status
/// route wants — unlike every other time in this API. It is decoded into a Go
/// `time.Time`, which insists on the colon in the zone offset: "+0300" fails
/// to parse and comes back as "invalid or missing custom_status".
pub fn rfc3339_local(millis: Millis) -> Option<String> {
    local(millis).map(|at| at.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

/// Reads the timestamp a custom status carries.
fn parse_expiry(expires_at: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(expires_at)
        .ok()
        .map(|at| at.with_timezone(&Local))
}

/// Whether a custom status has run out. Expired statuses stay on the user
/// object until the server clears them, so an old "on holiday" would otherwise
/// sit next to someone's name for weeks after they came back.
pub fn has_expired(expires_at: &str) -> bool {
    parse_expiry(expires_at).is_some_and(|when| when < Local::now())
}

/// "Until 15:30" for today, "Until tomorrow at 15:30", "Until Friday at
/// 15:30" within the week, and a date beyond that — the same ladder the web
/// client walks, because "until 15:30" is useless if it means next Thursday.
pub fn expiry_phrase(expires_at: &str) -> Option<String> {
    if expires_at.is_empty() {
        return None;
    }
    let when = parse_expiry(expires_at)?;
    let now = Local::now();
    // Already gone: the caller drops the whole chip in that case, but a stale
    // one must never claim a time in the past.
    if when <= now {
        return None;
    }

    let time = when.format("%H:%M").to_string();
    let days = when.ordinal() as i64 - now.ordinal() as i64;
    // Across a year boundary the day numbers reset, so fall through to the
    // date rather than reporting a negative difference.
    let phrase = match days {
        0 if when.year() == now.year() => format!("Until {time}"),
        1 if when.year() == now.year() => format!("Until tomorrow at {time}"),
        2..=6 if when.year() == now.year() => {
            format!("Until {} at {time}", when.format("%A"))
        }
        _ => format!("Until {}", when.format("%-d %B")),
    };
    Some(phrase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_hours(hours: i64) -> String {
        (Local::now() + chrono::Duration::hours(hours))
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string()
    }

    #[test]
    fn says_when_it_clears() {
        // An hour out is a time; four days out has to name the day, or
        // "until 15:30" reads as this afternoon.
        assert!(expiry_phrase(&in_hours(1)).unwrap().starts_with("Until "));
        let far = expiry_phrase(&in_hours(24 * 4)).unwrap();
        assert!(far.contains(" at "), "got {far:?}");

        // Nothing to say, and nothing to claim.
        assert_eq!(expiry_phrase(""), None);
        assert_eq!(expiry_phrase("not a date"), None);
        // In the past: never phrased as though it were still coming.
        assert_eq!(expiry_phrase(&in_hours(-2)), None);
    }

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
            DateTime::parse_from_rfc3339(&stamp).unwrap().timestamp_millis(),
            1_700_000_000_000
        );
    }

    #[test]
    fn days_are_named_relative_to_today() {
        assert_eq!(format_day(now_ms()), "Today");
        assert_eq!(format_day(now_ms() - 24 * 3_600_000), "Yesterday");
        // No padded single-digit day in the long form.
        assert!(!format_day(1_700_000_000_000).contains("  "));
    }

    #[test]
    fn unique_numbers_are() {
        let seen: std::collections::HashSet<i64> = (0..1000).map(|_| unique()).collect();
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn an_expired_status_is_recognised() {
        assert!(has_expired(&in_hours(-1)));
        assert!(!has_expired(&in_hours(1)));
        assert!(!has_expired(""));
    }
}
