use mattermost_api::models::Reaction;

use crate::state::AppState;

/// Who reacted with one emoji: names in the order they reacted, with the
/// current user pulled to the front as "You" — the same shape the webapp
/// builds for its own reaction tooltip (`reaction_tooltip/index.ts`'s
/// `getNamesOfUsers`) — plus how many more reacted whose profile we do not
/// have loaded, so a reactor we cannot name still counts instead of quietly
/// vanishing from the total.
pub(crate) fn reaction_names(reactions: &[&Reaction], st: &AppState) -> (Vec<String>, usize) {
    let mut ordered: Vec<&Reaction> = reactions.to_vec();
    ordered.sort_by_key(|r| r.create_at);

    let display = st.teammate_name_display().to_string();
    let mut you_reacted = false;
    let mut names = Vec::new();
    let mut unresolved = 0;
    for reaction in ordered {
        if reaction.user_id == st.me.id {
            you_reacted = true;
            continue;
        }
        match st.users.get(&reaction.user_id) {
            Some(user) => names.push(user.display_name(&display)),
            None => unresolved += 1,
        }
    }
    if you_reacted {
        names.insert(0, "You".to_string());
    }
    (names, unresolved)
}
