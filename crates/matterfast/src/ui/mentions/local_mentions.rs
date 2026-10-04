use std::sync::Arc;

use gpui_kit::RenderImage;
use mattermost_api::models::User;

use crate::ui::autocomplete::Candidate;
use crate::ui::constants::COMPLETIONS;

/// How the match was made, and therefore how it sorts.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    HandlePrefix,
    NamePrefix,
    Contains,
}

/// Mentions answerable from memory: anyone whose handle or name *contains*
/// what has been typed.
///
/// Substring, not prefix — people search by surname ("fomche" for Semyon
/// Fomchenkov) far more often than by the start of a handle. Ranked so the
/// prefix matches still come first, because when the prefix is what was meant
/// it is nearly always the one wanted.
///
/// The scan is linear over the directory: username, nickname, first and last
/// name are each lowercased and searched independently, because the server's
/// teammate-name-display setting only picks what is *shown* — a server set to
/// show bare usernames still has to be searchable by surname.
/// `picture` is asked for a face only for the handful of people that survive
/// the filter — it is a side effect (a missing avatar starts a download), so
/// it must not run for the whole directory, and tests pass one that does
/// nothing.
pub(crate) fn local_mentions<'a>(
    users: impl Iterator<Item = &'a User>,
    lowered: &str,
    display: &str,
    picture: &dyn Fn(&str) -> Option<Arc<RenderImage>>,
) -> Vec<Candidate> {
    let mut found: Vec<(Rank, String, &User)> = Vec::new();
    for user in users {
        let handle = user.username.to_lowercase();
        let nickname = user.nickname.to_lowercase();
        let first = user.first_name.to_lowercase();
        let last = user.last_name.to_lowercase();
        // What the row shows respects the display setting; what it is found
        // by does not.
        let shown = user.display_name(display);

        let name_prefix = nickname.starts_with(lowered)
            || first.starts_with(lowered)
            || last.starts_with(lowered);
        let name_contains =
            nickname.contains(lowered) || first.contains(lowered) || last.contains(lowered);

        let rank = if handle.starts_with(lowered) {
            Rank::HandlePrefix
        } else if name_prefix {
            Rank::NamePrefix
        } else if handle.contains(lowered) || name_contains {
            Rank::Contains
        } else {
            continue;
        };
        found.push((rank, shown, user));
    }

    // Sorted rather than truncated early: a substring match found first must
    // not push out a prefix match found later.
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    found
        .into_iter()
        .take(COMPLETIONS)
        .map(|(_, name, user)| Candidate {
            insert: format!("@{}", user.username),
            primary: name,
            secondary: format!("@{}", user.username),
            emoji: None,
            image: picture(&user.id),
            user_id: Some(user.id.clone()),
        })
        .collect()
}
