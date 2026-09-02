//! Profile picture cache.
//!
//! `gdk::Texture` is not `Send` and the download must not block the UI, so the
//! split is: fetch bytes on Tokio, decode into a texture on the GTK thread,
//! keep it in a plain `RefCell` map. Everything here therefore runs on the main
//! thread and needs no locking beyond that.
//!
//! The server always returns *an* image — it renders default initials for users
//! who never uploaded one — so there is no "user has no avatar" case to handle.
//! There is only "not fetched yet", which shows libadwaita's own initials until
//! the picture lands.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk::{gdk, glib};
use mattermost_api::Client;

use crate::runtime;

#[derive(Default)]
struct Inner {
    textures: HashMap<String, gdk::Texture>,
    /// In flight, so redrawing a hundred messages does not start a hundred
    /// downloads of the same face.
    pending: HashSet<String>,
    /// Fetches that failed. Retried only via [`Avatars::forget`], so a broken
    /// avatar cannot turn into a request loop driven by every redraw.
    failed: HashSet<String>,
}

/// Fired on the GTK thread when a texture lands.
type LoadedCallback = Rc<RefCell<Option<Box<dyn Fn()>>>>;

/// Marks a cache key as a file thumbnail rather than a user's picture.
const FILE_PREFIX: &str = "file:";
/// Marks a cache key as a file's larger preview image, on the same cache but
/// never colliding with its own thumbnail entry.
const FILE_PREVIEW_PREFIX: &str = "preview:";
/// Marks a cache key as a custom emoji, looked up by name rather than by id.
const EMOJI_PREFIX: &str = "emoji:";

#[derive(Clone)]
pub struct Avatars {
    inner: Rc<RefCell<Inner>>,
    client: Client,
    /// Notifies the view that it should redraw. Debouncing is the caller's
    /// business.
    on_loaded: LoadedCallback,
}

impl Avatars {
    pub fn new(client: Client) -> Self {
        Avatars {
            inner: Rc::new(RefCell::new(Inner::default())),
            client,
            on_loaded: Rc::new(RefCell::new(None)),
        }
    }

    pub fn connect_loaded(&self, f: impl Fn() + 'static) {
        *self.on_loaded.borrow_mut() = Some(Box::new(f));
    }

    /// The texture for a user if we have it; otherwise starts a fetch and
    /// returns `None` so the caller can draw initials meanwhile.
    pub fn texture(&self, user_id: &str) -> Option<gdk::Texture> {
        self.cached(user_id)
    }

    /// A custom emoji's image, on the same cache as everything else here.
    /// Custom emoji are per-server uploads, so there is no local table to fall
    /// back to — either the picture arrives or the shortcode stands in.
    pub fn custom_emoji(&self, name: &str) -> Option<gdk::Texture> {
        self.cached(&format!("{EMOJI_PREFIX}{name}"))
    }

    /// The thumbnail for an attached image, on the same cache and the same
    /// "ask once, redraw when it lands" contract as a face. Keyed apart so a
    /// file id can never collide with a user id.
    pub fn file_thumbnail(&self, file_id: &str) -> Option<gdk::Texture> {
        self.cached(&format!("{FILE_PREFIX}{file_id}"))
    }

    /// An attached image's larger preview (server-capped at 1920px wide),
    /// for showing it inline without either a blurry thumbnail or a full
    /// download of the original.
    pub fn file_preview(&self, file_id: &str) -> Option<gdk::Texture> {
        self.cached(&format!("{FILE_PREVIEW_PREFIX}{file_id}"))
    }

    fn cached(&self, key: &str) -> Option<gdk::Texture> {
        if let Some(texture) = self.inner.borrow().textures.get(key) {
            return Some(texture.clone());
        }
        self.request(key);
        None
    }

    /// Fills an avatar widget, falling back to initials until the picture
    /// arrives.
    pub fn apply(&self, avatar: &adw::Avatar, user_id: &str, display_name: &str) {
        avatar.set_text(Some(display_name));
        avatar.set_show_initials(true);
        match self.texture(user_id) {
            Some(texture) => avatar.set_custom_image(Some(&texture)),
            None => avatar.set_custom_image(gdk::Paintable::NONE),
        }
    }

    /// Drops a cached picture so the next request refetches — used when a
    /// user's `last_picture_update` moves.
    pub fn forget(&self, user_id: &str) {
        let mut inner = self.inner.borrow_mut();
        inner.textures.remove(user_id);
        inner.failed.remove(user_id);
    }

    fn request(&self, user_id: &str) {
        {
            let mut inner = self.inner.borrow_mut();
            if inner.pending.contains(user_id) || inner.failed.contains(user_id) {
                return;
            }
            inner.pending.insert(user_id.to_string());
        }

        let client = self.client.clone();
        let fetch_id = user_id.to_string();
        let done_id = user_id.to_string();
        let this = self.clone();

        runtime::spawn(
            async move {
                if let Some(file_id) = fetch_id.strip_prefix(FILE_PREFIX) {
                    return client.file_thumbnail_bytes(file_id).await;
                }
                if let Some(file_id) = fetch_id.strip_prefix(FILE_PREVIEW_PREFIX) {
                    return client.file_preview_bytes(file_id).await;
                }
                if let Some(name) = fetch_id.strip_prefix(EMOJI_PREFIX) {
                    // Two calls: the name has to become an id before the image
                    // can be asked for.
                    let emoji = client.emoji_by_name(name).await?;
                    return client.emoji_image_bytes(&emoji.id).await;
                }
                client.user_image_bytes(&fetch_id).await
            },
            move |result| {
                let mut loaded = false;
                {
                    let mut inner = this.inner.borrow_mut();
                    inner.pending.remove(&done_id);
                    match result {
                        Ok(bytes) => {
                            match gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)) {
                                Ok(texture) => {
                                    inner.textures.insert(done_id, texture);
                                    loaded = true;
                                }
                                // A shortcode that is neither Unicode nor uploaded to
                                // this server is an ordinary thing for someone to
                                // type, not a failure worth the word "failed".
                                Err(e) if done_id.starts_with(EMOJI_PREFIX) => {
                                    tracing::debug!(
                                        emoji = done_id.trim_start_matches(EMOJI_PREFIX),
                                        "no such custom emoji on this server"
                                    );
                                    let _ = e;
                                    inner.failed.insert(done_id.clone());
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "undecodable avatar image");
                                    inner.failed.insert(done_id);
                                }
                            }
                        }
                        // A shortcode that is neither Unicode nor uploaded to
                        // this server is an ordinary thing for someone to
                        // type, not a failure worth the word "failed".
                        Err(e) if done_id.starts_with(EMOJI_PREFIX) => {
                            tracing::debug!(
                                emoji = done_id.trim_start_matches(EMOJI_PREFIX),
                                "no such custom emoji on this server"
                            );
                            let _ = e;
                            inner.failed.insert(done_id.clone());
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, "avatar fetch failed");
                            inner.failed.insert(done_id);
                        }
                    }
                }
                // The borrow is released before the callback: it will almost
                // certainly redraw, and redrawing reads this same map.
                if loaded {
                    if let Some(callback) = this.on_loaded.borrow().as_ref() {
                        callback();
                    }
                }
            },
        );
    }
}
