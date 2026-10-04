use chrono::{Days, Months, NaiveDate};

use super::suggestion::Suggestion;

/// The days people mean most often, for the date modifiers. The server wants
/// `YYYY-MM-DD`; a row is a way not to have to work one out. Nothing once a
/// whole date has been typed — there is nothing left to suggest.
pub(super) fn dates(modifier: &str, typed: &str, today: NaiveDate) -> Vec<Suggestion> {
    if NaiveDate::parse_from_str(typed, "%Y-%m-%d").is_ok() {
        return Vec::new();
    }
    let days = [
        ("Today", Some(today)),
        ("Yesterday", today.checked_sub_days(Days::new(1))),
        ("A week ago", today.checked_sub_days(Days::new(7))),
        ("A month ago", today.checked_sub_months(Months::new(1))),
    ];
    days.into_iter()
        .filter_map(|(label, day)| Some((label, day?.format("%Y-%m-%d").to_string())))
        .filter(|(label, day)| day.starts_with(typed) || label.to_lowercase().starts_with(typed))
        .map(|(label, day)| Suggestion {
            insert: format!("{modifier}{day}"),
            label: label.to_string(),
            detail: day,
            open_ended: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(rows: &[Suggestion]) -> Vec<&str> {
        rows.iter().map(|row| row.label.as_str()).collect()
    }

    #[test]
    fn the_days_people_mean_are_offered_until_a_date_is_typed() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let rows = dates("before:", "", today);
        assert_eq!(
            labels(&rows),
            ["Today", "Yesterday", "A week ago", "A month ago"]
        );
        assert_eq!(rows[0].insert, "before:2026-03-01");
        assert_eq!(rows[1].insert, "before:2026-02-28");
        assert_eq!(rows[3].insert, "before:2026-02-01");
        // By the name of the day or by the beginning of the date.
        assert_eq!(labels(&dates("on:", "yes", today)), ["Yesterday"]);
        assert_eq!(labels(&dates("on:", "2026-03", today)), ["Today"]);
        assert!(dates("on:", "2025-12-24", today).is_empty());
    }
}
