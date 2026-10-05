use std::collections::HashMap;

use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    /// Channels of the current team, ordered the way the sidebar wants them,
    /// grouped by category. Borrowed: this is asked for on every draw of the
    /// channel list.
    pub fn sidebar_groups(&self) -> Vec<(&SidebarCategory, Vec<&Channel>)> {
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
            let mut channels: Vec<&Channel> = category
                .channel_ids
                .iter()
                .filter_map(|id| self.channels.get(id))
                .filter(|c| !c.is_archived())
                .collect();

            mattermost_api::bootstrap::sort_channel_refs(
                &mut channels,
                |id| self.memberships.get(id).is_some_and(ChannelMember::is_muted),
                &category.sorting,
                &category.channel_ids,
            );
            if !channels.is_empty() {
                out.push((category, channels));
            }
        }
        out
    }
}
