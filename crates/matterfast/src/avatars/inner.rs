use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::RenderImage;

use super::constants::{MAX_HEADS, MAX_TEXTURE_BYTES};
use super::texture_bytes::texture_bytes;

#[derive(Default)]
pub(super) struct Inner {
    pub(super) textures: HashMap<String, Arc<RenderImage>>,
    /// The head of a video file — not a decoded image, so it sits beside
    /// `textures` rather than in it. See `Avatars::video_head`.
    pub(super) heads: HashMap<String, Rc<Vec<u8>>>,
    /// In flight, so redrawing a hundred messages does not start a hundred
    /// downloads of the same face.
    pub(super) pending: HashSet<String>,
    /// Fetches that failed. Retried only via `Avatars::forget`, so a broken
    /// avatar cannot turn into a request loop driven by every redraw.
    pub(super) failed: HashSet<String>,
    /// When each texture was last asked for, so that what goes when the cache
    /// is full is what nobody has drawn for the longest. Going by when a
    /// picture arrived instead dropped the faces on screen — the first things
    /// in — to make room for a screenshot scrolling past, and every one of
    /// them was fetched again and the whole window drawn again for it.
    pub(super) seen: HashMap<String, Cell<Instant>>,
    /// What the textures add up to, kept as they go in and out rather than
    /// recomputed: the answer is wanted on every insert.
    pub(super) held: usize,
}

/// The key that has gone longest without being asked for.
fn longest_unseen(seen: &HashMap<String, Cell<Instant>>) -> Option<String> {
    seen.iter()
        .min_by_key(|(_, at)| at.get())
        .map(|(key, _)| key.clone())
}

impl Inner {
    /// A texture, noted as wanted just now. The note is a `Cell` so that the
    /// read path — every redraw — stays a read.
    pub(super) fn texture(&self, key: &str) -> Option<Arc<RenderImage>> {
        let texture = self.textures.get(key)?;
        if let Some(seen) = self.seen.get(key) {
            seen.set(Instant::now());
        }
        Some(texture.clone())
    }

    /// Takes a texture out, and what is kept about it with it.
    pub(super) fn remove(&mut self, key: &str) -> Option<Arc<RenderImage>> {
        self.seen.remove(key);
        let texture = self.textures.remove(key)?;
        self.held = self.held.saturating_sub(texture_bytes(&texture));
        Some(texture)
    }

    /// Keeps the caches to their bounds. Called after every insert. Hands back
    /// what it dropped, because the renderer has to be told as well: it keeps
    /// whatever it uploaded until asked to let go.
    #[must_use]
    pub(super) fn trim(&mut self) -> Vec<Arc<RenderImage>> {
        let mut dropped = Vec::new();
        while self.heads.len() > MAX_HEADS {
            let Some(oldest) = self.heads.keys().next().cloned() else {
                break;
            };
            self.heads.remove(&oldest);
        }
        while self.held > MAX_TEXTURE_BYTES {
            let Some(unseen) = longest_unseen(&self.seen) else {
                break;
            };
            if let Some(texture) = self.remove(&unseen) {
                tracing::debug!(
                    held_mb = self.held / (1024 * 1024),
                    "picture cache full, dropped the one longest unseen"
                );
                dropped.push(texture);
            }
        }
        dropped
    }

    /// Files a picture under `key`, returning whatever that pushed out.
    #[must_use]
    pub(super) fn insert(&mut self, key: String, texture: Arc<RenderImage>) -> Vec<Arc<RenderImage>> {
        let mut dropped = Vec::new();
        dropped.extend(self.remove(&key));
        self.held += texture_bytes(&texture);
        // It was asked for, which is why it was fetched.
        self.seen.insert(key.clone(), Cell::new(Instant::now()));
        self.textures.insert(key, texture);
        dropped.extend(self.trim());
        dropped
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use super::longest_unseen;

    #[test]
    fn what_goes_first_is_what_was_asked_for_longest_ago_whenever_it_arrived() {
        let now = Instant::now();
        let mut seen = HashMap::new();
        // The face came first and is still being drawn; the screenshot came
        // later and has scrolled away.
        seen.insert("face".to_string(), Cell::new(now));
        seen.insert(
            "file:screenshot".to_string(),
            Cell::new(now - Duration::from_secs(30)),
        );
        assert_eq!(longest_unseen(&seen).as_deref(), Some("file:screenshot"));
    }

    #[test]
    fn asking_again_spares_a_picture() {
        let now = Instant::now();
        let mut seen = HashMap::new();
        seen.insert("a".to_string(), Cell::new(now - Duration::from_secs(30)));
        seen.insert("b".to_string(), Cell::new(now - Duration::from_secs(20)));
        seen["a"].set(now);
        assert_eq!(longest_unseen(&seen).as_deref(), Some("b"));
    }

    #[test]
    fn an_empty_cache_has_nothing_to_give_up() {
        assert_eq!(longest_unseen(&HashMap::new()), None);
    }
}
