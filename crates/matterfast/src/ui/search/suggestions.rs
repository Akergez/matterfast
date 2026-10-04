use chrono::NaiveDate;

use super::channels::channels;
use super::dates::dates;
use super::hint::Hint;
use super::modifiers::modifiers;
use super::people::people;
use super::suggestion::Suggestion;
use crate::state::AppState;

/// The rows for a hint, from what is already known here.
pub(super) fn suggestions(st: &AppState, hint: &Hint, today: NaiveDate) -> Vec<Suggestion> {
    match hint {
        Hint::Modifiers { typed, first } => modifiers(typed, *first),
        Hint::From(typed) => people(st, typed),
        Hint::In(typed) => channels(st, typed),
        Hint::Date(modifier, typed) => dates(modifier, typed, today),
    }
}

/// What the list is headed with.
pub(super) fn heading(hint: &Hint) -> &'static str {
    match hint {
        Hint::Modifiers { .. } => "Search options",
        Hint::From(_) => "From",
        Hint::In(_) => "In",
        Hint::Date(..) => "Date, as YYYY-MM-DD",
    }
}
