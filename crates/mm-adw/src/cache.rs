//! What the last session looked like, so the next one has something to draw
//! before the network answers.
//!
//! Not a database and not a source of truth — a snapshot. Everything in it is
//! replaced the moment the real data lands; it exists so that opening the app
//! shows your channels and your last conversation immediately instead of a
//! spinner, which is the difference between a client that feels instant and
//! one that is merely fast.
//!
//! ponytail: one JSON file, rewritten whole. A real store (SQLite, per-channel
//! rows) is the upgrade if this ever holds more than the current team's
//! channels and one screenful of posts each.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use gtk::glib;
use mattermost_api::models::*;
use serde::{Deserialize, Serialize};

/// How many posts per channel are worth keeping. Enough to fill a window, not
/// enough to make the file worth attacking.
const POSTS_PER_CHANNEL: usize = 40;
/// Channels whose feed is kept. The rest come back from the network.
const FEEDS: usize = 8;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// Which server this belongs to. A cache from a different server is
    /// someone else's data and is dropped rather than shown.
    pub server: String,
    pub me: Option<User>,
    #[serde(default)]
    pub teams: Vec<Team>,
    #[serde(default)]
    pub current_team: Option<String>,
    #[serde(default)]
    pub current_channel: Option<String>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub memberships: Vec<ChannelMember>,
    #[serde(default)]
    pub categories: OrderedSidebarCategories,
    #[serde(default)]
    pub users: Vec<User>,
    /// Channel id → its most recent posts, oldest first.
    #[serde(default)]
    pub feeds: HashMap<String, Vec<Post>>,
}

fn path() -> PathBuf {
    glib::user_cache_dir()
        .join(crate::APP_ID)
        .join("snapshot.json")
}

/// Reads the snapshot, if it belongs to this server.
pub fn load(server: &str) -> Option<Snapshot> {
    let raw = fs::read_to_string(path()).ok()?;
    let snapshot: Snapshot = serde_json::from_str(&raw).ok()?;
    (snapshot.server == server).then_some(snapshot)
}

/// Replaces the snapshot. Failures are logged and otherwise ignored: a cache
/// that cannot be written is a slower next launch, not an error.
pub fn save(snapshot: &Snapshot) {
    let path = path();
    if let Some(dir) = path.parent() {
        if let Err(e) = fs::create_dir_all(dir) {
            tracing::warn!(error = %e, "could not create the cache directory");
            return;
        }
    }
    // 0600 from the start: this holds messages, and a file created world
    // readable is readable for however long it takes to chmod it.
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path);
    let mut file = match file {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "could not write the cache");
            return;
        }
    };
    let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    match serde_json::to_vec(snapshot) {
        Ok(bytes) => {
            if let Err(e) = file.write_all(&bytes) {
                tracing::warn!(error = %e, "could not write the cache");
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not serialise the cache"),
    }
}

/// Removes it. Called on sign-out: the next person to use this account should
/// not find the last one's messages.
pub fn clear() {
    let _ = fs::remove_file(path());
}

/// Trims a set of feeds down to what is worth storing: the channels most
/// recently posted in, and only the tail of each.
pub fn trim_feeds<'a>(
    feeds: impl Iterator<Item = (&'a String, &'a [Post])>,
    current: Option<&str>,
) -> HashMap<String, Vec<Post>> {
    let mut kept: Vec<(&String, &[Post])> = feeds.collect();
    // The channel being read is the one whose absence would be noticed, so it
    // is kept whatever its position.
    kept.sort_by_key(|(id, posts)| {
        let is_current = current == Some(id.as_str());
        let newest = posts.last().map(|p| p.create_at).unwrap_or(0);
        (!is_current, std::cmp::Reverse(newest))
    });
    kept.into_iter()
        .take(FEEDS)
        .map(|(id, posts)| {
            let start = posts.len().saturating_sub(POSTS_PER_CHANNEL);
            (id.clone(), posts[start..].to_vec())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: &str, at: Millis) -> Post {
        Post {
            id: id.into(),
            create_at: at,
            ..Default::default()
        }
    }

    #[test]
    fn only_the_tail_of_each_feed_is_kept() {
        let long: Vec<Post> = (0..100).map(|i| post(&i.to_string(), i)).collect();
        let id = "c1".to_string();
        let feeds = vec![(&id, long.as_slice())];
        let trimmed = trim_feeds(feeds.into_iter(), None);
        let kept = &trimmed["c1"];
        assert_eq!(kept.len(), POSTS_PER_CHANNEL);
        // The tail, not the head: the newest posts are the ones worth showing.
        assert_eq!(kept.last().unwrap().id, "99");
    }

    #[test]
    fn the_open_channel_survives_however_stale_it_is() {
        let ids: Vec<String> = (0..12).map(|i| format!("c{i}")).collect();
        let feeds: Vec<(&String, Vec<Post>)> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id, vec![post("p", i as Millis)]))
            .collect();
        let borrowed: Vec<(&String, &[Post])> =
            feeds.iter().map(|(id, p)| (*id, p.as_slice())).collect();

        // c0 is the oldest of twelve and would fall outside the limit.
        let trimmed = trim_feeds(borrowed.into_iter(), Some("c0"));
        assert_eq!(trimmed.len(), FEEDS);
        assert!(trimmed.contains_key("c0"));
    }
}
