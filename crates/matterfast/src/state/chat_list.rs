use std::collections::HashSet;

use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    /// The folders a list of conversations can be narrowed to: the server's
    /// sidebar categories, in its order, leaving out the ones with nothing
    /// in them.
    ///
    /// A category was a heading in a long list. Here it is a folder beside
    /// "all", which is the folders one after another.
    ///
    /// Asked for on every frame the tabs are drawn in, so it does not sort
    /// anything: which categories have a channel is all it needs to know.
    pub fn folders(&self) -> Vec<&SidebarCategory> {
        self.ordered_categories()
            .into_iter()
            .filter(|category| self.channels_of(category).next().is_some())
            .collect()
    }

    /// The conversations of the current team, in no order, each once: for
    /// what picks some of them out and has an order of its own to put them
    /// in. [`chat_list`](Self::chat_list) is these, in the server's order.
    pub fn chats(&self) -> impl Iterator<Item = &Channel> {
        let mut seen = HashSet::new();
        self.categories
            .categories
            .iter()
            .flat_map(|category| self.channels_of(category))
            .filter(move |channel| seen.insert(channel.id.as_str()))
    }

    /// The conversations of the current team — channels and direct messages
    /// alike — category after category, each in the order its own sorting
    /// gives it: the order the person arranged them in, and the one they
    /// know where to look in. `folder` is a category's id; one that is not
    /// there is the same as none, so a folder that was deleted while it was
    /// open shows everything rather than nothing.
    ///
    /// Nothing here moves when somebody writes: what is new and what is
    /// waiting on the reader is the inbox, and a list that reshuffled itself
    /// by time beside it would only be a worse copy of that.
    pub fn chat_list(&self, folder: Option<&str>) -> Vec<&Channel> {
        let groups = self.sidebar_groups();
        let folder = folder.filter(|id| groups.iter().any(|(category, _)| category.id == *id));
        let mut seen = HashSet::new();
        groups
            .into_iter()
            .filter(|(category, _)| folder.is_none_or(|id| category.id == id))
            .flat_map(|(_, channels)| channels)
            .filter(|channel| seen.insert(channel.id.as_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use mattermost_api::Client;

    use super::*;

    fn state() -> AppState {
        let client = Client::new("http://x.test").unwrap();
        let mut st = AppState::new(client, User::default(), ClientConfig::default(), false);
        for (id, written) in [("old", 100), ("new", 300), ("mid", 200), ("never", 0)] {
            st.channels.insert(
                id.into(),
                Channel { id: id.into(), last_post_at: written, ..Default::default() },
            );
        }
        let category = |id: &str, channels: &[&str]| SidebarCategory {
            id: id.into(),
            display_name: id.into(),
            channel_ids: channels.iter().map(|id| id.to_string()).collect(),
            ..Default::default()
        };
        st.categories = OrderedSidebarCategories {
            order: vec!["work".into(), "people".into(), "empty".into()],
            categories: vec![
                category("work", &["old", "new"]),
                category("people", &["mid", "never"]),
                category("empty", &[]),
            ],
        };
        st
    }

    fn ids(chats: Vec<&Channel>) -> Vec<&str> {
        chats.into_iter().map(|channel| channel.id.as_str()).collect()
    }

    #[test]
    fn every_conversation_is_in_one_list_in_the_order_of_its_categories_not_of_time() {
        let st = state();
        assert_eq!(ids(st.chat_list(None)), ["old", "new", "mid", "never"]);
    }

    #[test]
    fn a_folder_holds_only_its_own_in_the_same_order() {
        let st = state();
        assert_eq!(ids(st.chat_list(Some("work"))), ["old", "new"]);
        assert_eq!(ids(st.chat_list(Some("people"))), ["mid", "never"]);
    }

    #[test]
    fn a_folder_that_is_gone_shows_everything() {
        let st = state();
        assert_eq!(st.chat_list(Some("deleted")).len(), 4);
    }

    #[test]
    fn a_category_with_nothing_in_it_is_not_a_folder() {
        let st = state();
        let folders: Vec<_> = st.folders().into_iter().map(|c| c.id.as_str()).collect();
        assert_eq!(folders, ["work", "people"]);
    }
}
