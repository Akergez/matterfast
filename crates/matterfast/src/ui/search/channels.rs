use mattermost_api::models::ChannelType;

use super::constants::ROWS;
use super::suggestion::Suggestion;
use crate::state::AppState;

/// The channels of this team, and the direct messages, that `typed` is in
/// the name of. A channel is searched by its URL name; a direct message by
/// the other person's handle, which the server spells `@handle`.
pub(super) fn channels(st: &AppState, typed: &str) -> Vec<Suggestion> {
    let team = st.current_team.as_deref().unwrap_or_default();
    let mut found: Vec<(bool, Suggestion)> = st
        .channels
        .values()
        .filter(|channel| channel.delete_at == 0)
        .filter_map(|channel| {
            let (name, title) = match channel.r#type {
                ChannelType::Direct => {
                    let other = st.users.get(channel.dm_teammate_id(&st.me.id)?)?;
                    (format!("@{}", other.username), st.channel_title(channel))
                }
                ChannelType::Group => return None,
                _ if channel.team_id == team => {
                    (channel.name.clone(), channel.display_name.clone())
                }
                _ => return None,
            };
            let plain = name.trim_start_matches('@').to_string();
            let typed = typed.trim_start_matches('@');
            let begins = plain.starts_with(typed) || title.to_lowercase().starts_with(typed);
            let has = begins || plain.contains(typed) || title.to_lowercase().contains(typed);
            has.then(|| {
                let row = Suggestion {
                    insert: format!("in:{name}"),
                    label: title,
                    detail: name,
                    open_ended: false,
                };
                (!begins, row)
            })
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    found.into_iter().map(|(_, row)| row).take(ROWS).collect()
}
