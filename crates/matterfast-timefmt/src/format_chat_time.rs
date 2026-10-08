use chrono::{DateTime, Local};
use mattermost_api::models::Millis;

use crate::local::local;

/// When a conversation was last written in, as a list of conversations says
/// it: the time for today, the day of the week for the last few days, the
/// date before that. Nothing for a conversation nobody has written in.
///
/// Unlike [`format_relative`](crate::format_relative) it does not go stale
/// while it is on screen: "14:05" is right until midnight, and "5m" is wrong
/// a minute later.
pub fn format_chat_time(millis: Millis) -> String {
    chat_time(millis, Local::now())
}

fn chat_time(millis: Millis, now: DateTime<Local>) -> String {
    let Some(then) = local(millis).filter(|_| millis > 0) else {
        return String::new();
    };
    let days = (now.date_naive() - then.date_naive()).num_days();
    match days {
        ..=0 => then.format("%H:%M").to_string(),
        1..=6 => then.format("%a").to_string(),
        _ if then.format("%Y").to_string() == now.format("%Y").to_string() => {
            then.format("%-d %b").to_string()
        }
        _ => then.format("%-d %b %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    fn at(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(year, month, day, hour, 5, 0).unwrap()
    }

    #[test]
    fn today_is_a_time_and_the_last_few_days_are_a_weekday() {
        let now = at(2026, 10, 8, 18);
        assert_eq!(chat_time(at(2026, 10, 8, 9).timestamp_millis(), now), "09:05");
        // Yesterday evening is a day ago, however few hours that is.
        let yesterday = at(2026, 10, 7, 23);
        assert_eq!(chat_time(yesterday.timestamp_millis(), now), yesterday.format("%a").to_string());
        let six = now - Duration::days(6);
        assert_eq!(chat_time(six.timestamp_millis(), now), six.format("%a").to_string());
    }

    #[test]
    fn longer_ago_is_a_date_with_the_year_only_when_it_is_another() {
        let now = at(2026, 10, 8, 18);
        assert_eq!(chat_time(at(2026, 9, 3, 12).timestamp_millis(), now), "3 Sep");
        assert_eq!(chat_time(at(2025, 12, 31, 12).timestamp_millis(), now), "31 Dec 2025");
    }

    #[test]
    fn a_conversation_nobody_wrote_in_has_no_time() {
        assert_eq!(chat_time(0, at(2026, 10, 8, 18)), "");
    }
}
