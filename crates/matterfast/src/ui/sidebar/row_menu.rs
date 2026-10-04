use super::row_action::RowAction;
use crate::state::AppState;

/// What a channel row's menu offers, worked out from the state rather than
/// kept: whether it is read, whether it is muted, and where else it could go.
pub(super) fn row_menu(channel_id: &str, st: &AppState) -> Vec<(String, RowAction)> {
    let muted = st
        .memberships
        .get(channel_id)
        .is_some_and(|m| m.is_muted());
    let mut entries = Vec::new();
    if st.unread(channel_id).is_unread() {
        entries.push(("Mark as read".to_string(), RowAction::MarkRead));
    } else {
        entries.push(("Mark as unread".to_string(), RowAction::MarkUnread));
    }
    entries.push((
        if muted { "Unmute" } else { "Mute" }.to_string(),
        RowAction::SetMuted(!muted),
    ));

    // Where it is now is not somewhere to move it to.
    let current = st
        .categories
        .categories
        .iter()
        .find(|c| c.channel_ids.iter().any(|id| id == channel_id))
        .map(|c| c.id.clone())
        .unwrap_or_default();
    for category in &st.categories.categories {
        if category.id == current {
            continue;
        }
        entries.push((
            format!("Move to {}", category.display_name),
            RowAction::MoveTo(category.id.clone()),
        ));
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use mattermost_api::models::{Channel, ChannelMember, ClientConfig, SidebarCategory, User};
    use mattermost_api::Client;

    fn state() -> AppState {
        let client = Client::new("http://x.test").unwrap();
        let mut st = AppState::new(client, User::default(), ClientConfig::default(), false);
        st.channels.insert(
            "c1".into(),
            Channel {
                id: "c1".into(),
                total_msg_count: 10,
                ..Default::default()
            },
        );
        st.memberships.insert(
            "c1".into(),
            ChannelMember {
                channel_id: "c1".into(),
                msg_count: 10,
                ..Default::default()
            },
        );
        for (id, name, channels) in [
            ("fav", "Favorites", vec![]),
            ("chan", "Channels", vec!["c1".to_string()]),
        ] {
            st.categories.categories.push(SidebarCategory {
                id: id.into(),
                display_name: name.into(),
                channel_ids: channels,
                ..Default::default()
            });
        }
        st
    }

    fn labels(entries: &[(String, RowAction)]) -> Vec<&str> {
        entries.iter().map(|(label, _)| label.as_str()).collect()
    }

    #[test]
    fn the_menu_offers_the_opposite_of_what_is() {
        let mut st = state();
        assert_eq!(
            labels(&row_menu("c1", &st)),
            ["Mark as unread", "Mute", "Move to Favorites"]
        );

        // Something arrived.
        st.channels.get_mut("c1").unwrap().total_msg_count = 11;
        assert_eq!(
            labels(&row_menu("c1", &st)),
            ["Mark as read", "Mute", "Move to Favorites"]
        );

        // Muted, the same message no longer counts as unread — only a
        // mention would — so there is nothing left to mark read.
        st.memberships
            .get_mut("c1")
            .unwrap()
            .notify_props
            .insert("mark_unread".into(), "mention".into());
        assert_eq!(
            labels(&row_menu("c1", &st)),
            ["Mark as unread", "Unmute", "Move to Favorites"]
        );
    }

    #[test]
    fn a_channel_is_never_offered_a_move_to_where_it_already_is() {
        let st = state();
        assert!(!labels(&row_menu("c1", &st)).contains(&"Move to Channels"));
    }
}
