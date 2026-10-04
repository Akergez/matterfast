use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    /// The current team's URL name, for building permalinks.
    pub fn current_team_name(&self) -> Option<String> {
        let id = self.current_team.as_ref()?;
        self.teams
            .iter()
            .find(|t| &t.id == id)
            .map(|t| t.name.clone())
    }

    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.channels.get(id)
    }

    /// A channel's label, resolving DM/GM names to the people in them.
    ///
    /// DM channels carry `"<idA>__<idB>"` as their name and an empty display
    /// name, so the sidebar has to look the other person up itself.
    pub fn channel_title(&self, channel: &Channel) -> String {
        match channel.r#type {
            ChannelType::Direct => channel
                .dm_teammate_id(&self.me.id)
                .and_then(|id| self.users.get(id))
                .map(|u| u.display_name(self.teammate_name_display()))
                .unwrap_or_else(|| channel.display_name.clone()),
            _ => channel.display_name.clone(),
        }
    }

    pub fn teammate_name_display(&self) -> &str {
        self.config.get("TeammateNameDisplay").unwrap_or("username")
    }

    /// How a user should be named in this server's configured style.
    pub fn display_name(&self, user: &User) -> String {
        user.display_name(self.teammate_name_display())
    }

    /// Author label for a post, honouring webhook/bot overrides.
    pub fn author_name(&self, post: &Post) -> String {
        if let Some(name) = post.override_username() {
            return name.to_string();
        }
        self.users
            .get(&post.user_id)
            .map(|u| u.display_name(self.teammate_name_display()))
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// The group mentioned as `@name`, if there is one.
    pub fn group(&self, name: &str) -> Option<&Group> {
        self.groups.iter().find(|group| group.name == name)
    }
}
