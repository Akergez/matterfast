use mattermost_api::models::*;

use super::app_state::AppState;

impl AppState {
    /// Adds or removes a reaction wherever the post is held.
    pub fn apply_reaction(&mut self, reaction: &Reaction, added: bool) {
        let feeds = self.feeds.values_mut().chain(self.threads.values_mut());
        for feed in feeds {
            if let Some(post) = feed.posts.iter_mut().find(|p| p.id == reaction.post_id) {
                let metadata = post.metadata.get_or_insert_with(Default::default);
                if added {
                    let already = metadata.reactions.iter().any(|r| {
                        r.user_id == reaction.user_id && r.emoji_name == reaction.emoji_name
                    });
                    if !already {
                        metadata.reactions.push(reaction.clone());
                    }
                } else {
                    metadata.reactions.retain(|r| {
                        !(r.user_id == reaction.user_id && r.emoji_name == reaction.emoji_name)
                    });
                }
                post.has_reactions = !metadata.reactions.is_empty();
            }
        }
    }

    /// True when we already hold this reaction from ourselves — used to decide
    /// whether clicking a chip adds or removes.
    pub fn has_my_reaction(&self, post_id: &str, emoji_name: &str) -> bool {
        self.feeds
            .values()
            .chain(self.threads.values())
            .filter_map(|feed| feed.posts.iter().find(|p| p.id == post_id))
            .any(|post| {
                post.reactions()
                    .iter()
                    .any(|r| r.user_id == self.me.id && r.emoji_name == emoji_name)
            })
    }
}
