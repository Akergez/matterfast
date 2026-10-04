use chrono::{Datelike, Days, Local, NaiveDate, NaiveTime, TimeZone};

/// Now plus some minutes, in Unix milliseconds.
pub(crate) fn from_now(minutes: i64) -> i64 {
    (Local::now() + chrono::Duration::minutes(minutes)).timestamp_millis()
}

/// A local wall-clock moment in Unix milliseconds. `None` for a moment that
/// does not exist — the hour a clock change skips.
fn local_moment(day: NaiveDate, time: NaiveTime) -> Option<i64> {
    Local
        .from_local_datetime(&day.and_time(time))
        .earliest()
        .map(|moment| moment.timestamp_millis())
}

/// A typed date and time, in Unix milliseconds.
pub(crate) fn local_millis(date: &str, time: &str) -> Option<i64> {
    let day = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
    let time = NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()?;
    local_moment(day, time)
}

/// 9am local time, `days` days from today.
pub(crate) fn morning_in(days: u64) -> Option<i64> {
    let day = Local::now().date_naive().checked_add_days(Days::new(days))?;
    local_moment(day, NaiveTime::from_hms_opt(9, 0, 0)?)
}

pub(crate) fn next_monday_morning() -> Option<i64> {
    let today = Local::now().date_naive();
    morning_in(days_until_next_monday(today.weekday().number_from_monday()) as u64)
}

/// Days from `day_of_week` (1 = Monday … 7 = Sunday) to the next Monday.
/// Always in the future: asked on a Monday it means the next one, because
/// "Monday morning" is never the morning you are already in.
fn days_until_next_monday(day_of_week: u32) -> u32 {
    ((7 - day_of_week) % 7) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_monday_is_always_ahead() {
        assert_eq!(days_until_next_monday(1), 7); // Monday -> the one after
        assert_eq!(days_until_next_monday(2), 6); // Tuesday
        assert_eq!(days_until_next_monday(5), 3); // Friday
        assert_eq!(days_until_next_monday(7), 1); // Sunday -> tomorrow
    }

    #[test]
    fn a_typed_date_and_time_is_that_local_moment() {
        let millis = local_millis("2026-03-10", "14:30").expect("a real moment");
        let back = Local.timestamp_millis_opt(millis).single().unwrap();
        assert_eq!(
            back.format("%Y-%m-%d %H:%M").to_string(),
            "2026-03-10 14:30"
        );
        // Surrounding spaces are how a pasted value arrives.
        assert_eq!(local_millis(" 2026-03-10 ", " 14:30 "), Some(millis));

        // Not a date, not a time, and a day that does not exist.
        assert_eq!(local_millis("tomorrow", "14:30"), None);
        assert_eq!(local_millis("2026-03-10", "half past two"), None);
        assert_eq!(local_millis("2026-02-30", "09:00"), None);
    }

    #[test]
    fn the_usual_times_are_in_the_morning_and_ahead() {
        let now = Local::now().timestamp_millis();
        for millis in [morning_in(1).unwrap(), next_monday_morning().unwrap()] {
            assert!(millis > now);
            let at = Local.timestamp_millis_opt(millis).single().unwrap();
            assert_eq!(at.format("%H:%M").to_string(), "09:00");
        }
        assert!(from_now(30) > now);
    }
}
