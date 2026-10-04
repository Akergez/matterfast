use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

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
    /// Insertion order, so the oldest can be dropped when the cache is full.
    pub(super) order: Vec<String>,
    /// What the textures add up to, kept as they go in and out rather than
    /// recomputed: the answer is wanted on every insert.
    pub(super) held: usize,
}

impl Inner {
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
            let Some(oldest) = self.order.first().cloned() else {
                break;
            };
            self.order.remove(0);
            if let Some(texture) = self.textures.remove(&oldest) {
                self.held = self.held.saturating_sub(texture_bytes(&texture));
                tracing::debug!(
                    held_mb = self.held / (1024 * 1024),
                    "picture cache full, dropped the oldest"
                );
                dropped.push(texture);
            }
        }
        dropped
    }

    /// Files a picture under `key`, returning whatever that pushed out.
    #[must_use]
    pub(super) fn insert(&mut self, key: String, texture: Arc<RenderImage>) -> Vec<Arc<RenderImage>> {
        self.held += texture_bytes(&texture);
        self.order.retain(|other| other != &key);
        self.order.push(key.clone());
        let mut dropped = Vec::new();
        if let Some(old) = self.textures.insert(key, texture) {
            self.held = self.held.saturating_sub(texture_bytes(&old));
            dropped.push(old);
        }
        dropped.extend(self.trim());
        dropped
    }
}
