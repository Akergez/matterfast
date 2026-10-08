use std::collections::HashMap;

use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    /// Channels of the current team, ordered the way the sidebar wants them,
    /// grouped by category. Borrowed: this is asked for on every draw of the
    /// channel list.
    pub fn sidebar_groups(&self) -> Vec<(&SidebarCategory, Vec<&Channel>)> {
        let mut out = Vec::new();
        for category in self.ordered_categories() {
            let mut channels: Vec<&Channel> = self.channels_of(category).collect();

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

    /// The channels filed under a category that are still there to show, in
    /// no particular order: for what only needs to know which they are.
    pub fn channels_of<'a>(
        &'a self,
        category: &'a SidebarCategory,
    ) -> impl Iterator<Item = &'a Channel> {
        category
            .channel_ids
            .iter()
            .filter_map(|id| self.channels.get(id))
            .filter(|c| !c.is_archived())
    }

    /// The categories in the order the server gives them.
    pub fn ordered_categories(&self) -> Vec<&SidebarCategory> {
        let order = &self.categories.order;
        let by_id: HashMap<&str, &SidebarCategory> = self
            .categories
            .categories
            .iter()
            .map(|c| (c.id.as_str(), c))
            .collect();

        // `order` is authoritative; fall back to declaration order if it is
        // missing (older servers, or a partial sync).
        if order.is_empty() {
            self.categories.categories.iter().collect()
        } else {
            order
                .iter()
                .filter_map(|id| by_id.get(id.as_str()).copied())
                .collect()
        }
    }
}
