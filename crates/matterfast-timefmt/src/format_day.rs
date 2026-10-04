use chrono::{Days, Local};
use mattermost_api::models::Millis;

use crate::local::local;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::now_ms::now_ms;

    #[test]
    fn days_are_named_relative_to_today() {
        assert_eq!(format_day(now_ms()), "Today");
        assert_eq!(format_day(now_ms() - 24 * 3_600_000), "Yesterday");
        // No padded single-digit day in the long form.
        assert!(!format_day(1_700_000_000_000).contains("  "));
    }
}
