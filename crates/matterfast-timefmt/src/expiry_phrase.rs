use chrono::{Datelike, Local};

use crate::parse_expiry::parse_expiry;

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
    use crate::test_support::in_hours;

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
}
