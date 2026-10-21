//! The emoji somebody reacts with, remembered, so that the quick row of
//! reactions offers theirs rather than a fixed eight.
//!
//! The server keeps the list as a preference (`emoji` / `recent_emojis`), in
//! the webapp's shape: `[{"name": "tada", "usageCount": 3}, …]`, the latest
//! last. Keeping it there, and in that shape, is what makes the row the same
//! here as in the browser and on the phone, each of which adds to it too.

use mattermost_api::models::Preference;
use serde::{Deserialize, Serialize};

use super::app_state::AppState;
use crate::emoji::QUICK_REACTIONS;

const CATEGORY: &str = "emoji";
const NAME: &str = "recent_emojis";

/// How many the list keeps; the webapp's own limit.
const KEPT: usize = 27;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Recent {
    name: String,
    #[serde(rename = "usageCount", default)]
    usage_count: u32,
}

fn parse(value: Option<&str>) -> Vec<Recent> {
    value
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_default()
}

/// The quick row: the latest used first, then the usual ones it does not
/// already hold, as many as there are of those.
fn quick(value: Option<&str>) -> Vec<String> {
    let mut row: Vec<String> = Vec::with_capacity(QUICK_REACTIONS.len());
    let latest_first = parse(value).into_iter().rev().map(|recent| recent.name);
    let usual = QUICK_REACTIONS.iter().map(|name| name.to_string());
    for name in latest_first.chain(usual) {
        if row.len() == QUICK_REACTIONS.len() {
            break;
        }
        if !name.is_empty() && !row.contains(&name) {
            row.push(name);
        }
    }
    row
}

/// The list once `name` has been used again: moved to the end, counted.
fn used(value: Option<&str>, name: &str) -> String {
    let mut list = parse(value);
    let count = match list.iter().position(|recent| recent.name == name) {
        Some(index) => list.remove(index).usage_count,
        None => 0,
    };
    list.push(Recent { name: name.to_string(), usage_count: count + 1 });
    let excess = list.len().saturating_sub(KEPT);
    list.drain(..excess);
    serde_json::to_string(&list).expect("a list of names serialises")
}

impl AppState {
    fn recent_emoji(&self) -> Option<&str> {
        self.preferences
            .iter()
            .find(|p| p.category == CATEGORY && p.name == NAME)
            .map(|p| p.value.as_str())
    }

    /// The reactions offered one press away.
    pub fn quick_reactions(&self) -> Vec<String> {
        quick(self.recent_emoji())
    }

    /// Notes that `name` was reacted with, and answers the preference to
    /// tell the server.
    pub fn use_emoji(&mut self, name: &str) -> Preference {
        let preference = Preference {
            user_id: self.me.id.clone(),
            category: CATEGORY.into(),
            name: NAME.into(),
            value: used(self.recent_emoji(), name),
        };
        self.set_preferences(std::slice::from_ref(&preference));
        preference
    }

    /// Takes preferences the server says have changed, replacing what is held
    /// under the same category and name.
    pub fn set_preferences(&mut self, changed: &[Preference]) {
        for preference in changed {
            match self
                .preferences
                .iter_mut()
                .find(|p| p.category == preference.category && p.name == preference.name)
            {
                Some(held) => *held = preference.clone(),
                None => self.preferences.push(preference.clone()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{quick, used, KEPT};
    use crate::emoji::QUICK_REACTIONS;

    #[test]
    fn with_nothing_used_the_row_is_the_usual_one() {
        assert_eq!(quick(None), QUICK_REACTIONS);
        assert_eq!(quick(Some("not json")), QUICK_REACTIONS);
    }

    #[test]
    fn the_latest_used_come_first_and_the_usual_fill_the_rest() {
        let value = r#"[{"name":"party_parrot","usageCount":1},{"name":"heart","usageCount":5}]"#;
        let row = quick(Some(value));
        assert_eq!(row[..3], ["heart", "party_parrot", "+1"]);
        assert_eq!(row.len(), QUICK_REACTIONS.len());
        assert_eq!(row.iter().filter(|name| *name == "heart").count(), 1);
    }

    #[test]
    fn using_one_again_moves_it_last_and_counts_it() {
        let once = used(None, "tada");
        assert_eq!(once, r#"[{"name":"tada","usageCount":1}]"#);
        let twice = used(Some(&used(Some(&once), "eyes")), "tada");
        assert_eq!(
            twice,
            r#"[{"name":"eyes","usageCount":1},{"name":"tada","usageCount":2}]"#
        );
    }

    #[test]
    fn the_list_forgets_the_oldest_past_its_limit() {
        let mut value = None;
        for n in 0..KEPT + 3 {
            value = Some(used(value.as_deref(), &format!("e{n}")));
        }
        let value = value.unwrap();
        assert!(!value.contains(r#""e2""#));
        assert!(value.contains(r#""e3""#));
        assert_eq!(value.matches("name").count(), KEPT);
    }
}
