use super::constants::ROWS;
use super::suggestion::Suggestion;
use crate::state::AppState;

/// A person as a `from:` row.
pub fn person(username: &str, shown: &str) -> Suggestion {
    Suggestion {
        insert: format!("from:{username}"),
        label: shown.to_string(),
        detail: format!("@{username}"),
        open_ended: false,
    }
}

/// The people already known here whose handle or name has `typed` in it,
/// those it begins first.
pub(super) fn people(st: &AppState, typed: &str) -> Vec<Suggestion> {
    let display = st.teammate_name_display();
    let mut found: Vec<(bool, Suggestion)> = st
        .users
        .values()
        .filter(|user| user.delete_at == 0)
        .filter_map(|user| {
            let handle = user.username.to_lowercase();
            let shown = user.display_name(display);
            let begins = handle.starts_with(typed) || shown.to_lowercase().starts_with(typed);
            let names = [&user.first_name, &user.last_name, &user.nickname];
            let has = begins
                || handle.contains(typed)
                || names.iter().any(|name| name.to_lowercase().contains(typed));
            has.then(|| (!begins, person(&user.username, &shown)))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    found.into_iter().map(|(_, row)| row).take(ROWS).collect()
}
