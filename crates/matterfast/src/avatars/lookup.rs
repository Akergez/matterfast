use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{App, RenderImage};

use super::constants::{
    ANIMATION_PREFIX, EMOJI_PREFIX, FILE_PREFIX, FILE_PREVIEW_PREFIX, POSTER_PREFIX,
    VIDEO_HEAD_PREFIX,
};
use super::release::release;
use super::service::Avatars;

impl Avatars {
    /// The texture for a user if we have it; otherwise starts a fetch and
    /// returns `None` so the caller can draw initials meanwhile.
    pub fn texture(&self, user_id: &str) -> Option<Arc<RenderImage>> {
        self.cached(user_id)
    }

    /// A custom emoji's image, on the same cache as everything else here.
    /// Custom emoji are per-server uploads, so there is no local table to fall
    /// back to — either the picture arrives or the shortcode stands in.
    pub fn custom_emoji(&self, name: &str) -> Option<Arc<RenderImage>> {
        self.cached(&format!("{EMOJI_PREFIX}{name}"))
    }

    /// The thumbnail for an attached image, on the same cache and the same
    /// "ask once, redraw when it lands" contract as a face. Keyed apart so a
    /// file id can never collide with a user id.
    pub fn file_thumbnail(&self, file_id: &str) -> Option<Arc<RenderImage>> {
        self.cached(&format!("{FILE_PREFIX}{file_id}"))
    }

    /// An uploaded GIF, moving. The server's previews and thumbnails are
    /// stills, so this is the file itself — ask only for one small enough to
    /// be worth downloading unasked.
    pub fn file_animation(&self, file_id: &str) -> Option<Arc<RenderImage>> {
        self.cached(&format!("{ANIMATION_PREFIX}{file_id}"))
    }

    /// An attached image's larger preview (server-capped at 1920px wide),
    /// for showing it inline without either a blurry thumbnail or a full
    /// download of the original.
    pub fn file_preview(&self, file_id: &str) -> Option<Arc<RenderImage>> {
        self.cached(&format!("{FILE_PREVIEW_PREFIX}{file_id}"))
    }

    /// A video's still, if one has already been taken. Unlike everything else
    /// here it is never fetched — it is made locally from the head of the
    /// file and handed back with [`Avatars::remember_poster`] — so a miss
    /// starts nothing.
    pub fn poster(&self, file_id: &str) -> Option<Arc<RenderImage>> {
        self.inner
            .borrow()
            .texture(&format!("{POSTER_PREFIX}{file_id}"))
    }

    /// Keeps a still that was just taken, so the next redraw of that row
    /// draws a picture instead of starting a decoder over again.
    pub fn remember_poster(&self, file_id: &str, texture: Arc<RenderImage>, cx: &mut App) {
        let key = format!("{POSTER_PREFIX}{file_id}");
        let dropped = self.inner.borrow_mut().insert(key, texture);
        release(dropped, cx);
    }

    /// The first slice of a video file's bytes — not decoded into anything,
    /// just enough for the UI layer to judge whether a poster can be built
    /// from it. `None` while the fetch is in flight or has failed; a redraw
    /// triggered by [`connect_loaded`](Avatars::connect_loaded) is what
    /// picks it up once it lands.
    ///
    /// Cached like everything else here, so scrolling a video row out of view
    /// and back does not repeat the request.
    pub fn video_head(&self, file_id: &str) -> Option<Rc<Vec<u8>>> {
        // The prefix belongs in the lookup as well as the fetch. Without it
        // the hit never happened: every redraw asked again, every answer
        // redrew, and the two chased each other at the speed of the network
        // while the feed was rebuilt from scratch each time round.
        let key = format!("{VIDEO_HEAD_PREFIX}{file_id}");
        if let Some(bytes) = self.inner.borrow().heads.get(&key) {
            return Some(bytes.clone());
        }
        self.request(&key);
        None
    }

    fn cached(&self, key: &str) -> Option<Arc<RenderImage>> {
        if let Some(texture) = self.inner.borrow().texture(key) {
            return Some(texture);
        }
        self.request(key);
        None
    }
}
