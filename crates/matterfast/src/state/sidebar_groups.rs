use std::collections::HashMap;

use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    /// Channels of the current team, ordered the way the sidebar wants them,
    /// grouped by category.
    pub fn sidebar_groups(&self) -> Vec<(SidebarCategory, Vec<Channel>)> {
        let mut out = Vec::new();
        let order = &self.categories.order;
        let by_id: HashMap<&str, &SidebarCategory> = self
            .categories
            .categories
            .iter()
            .map(|c| (c.id.as_str(), c))
            .collect();

        // `order` is authoritative; fall back to declaration order if it is
        // missing (older servers, or a partial sync).
        let ordered: Vec<&SidebarCategory> = if order.is_empty() {
            self.categories.categories.iter().collect()
        } else {
            order
                .iter()
                .filter_map(|id| by_id.get(id.as_str()).copied())
                .collect()
        };

        for category in ordered {
            let mut channels: Vec<Channel> = category
                .channel_ids
                .iter()
                .filter_map(|id| self.channels.get(id))
                .filter(|c| !c.is_archived())
                .cloned()
                .collect();

            let members: Vec<ChannelMember> = channels
                .iter()
                .filter_map(|c| self.memberships.get(&c.id).cloned())
                .collect();

            mattermost_api::bootstrap::sort_channels(
                &mut channels,
                &members,
                &category.sorting,
                &category.channel_ids,
            );
            if !channels.is_empty() {
                out.push((category.clone(), channels));
            }
        }
        out
    }
}
