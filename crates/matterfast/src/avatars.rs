//! Profile picture cache.
//!
//! Neither the download nor the decode may block the UI, so both happen on
//! Tokio: the bytes are fetched, decoded and scaled there, and what comes back
//! to the main thread is a finished [`RenderImage`], kept in a plain `RefCell`
//! map. Everything that touches that map therefore runs on the main thread and
//! needs no locking beyond that.
//!
//! The server always returns *an* image — it renders default initials for users
//! who never uploaded one — so there is no "user has no avatar" case to handle.
//! There is only "not fetched yet", which shows initials until the picture
//! lands.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use std::sync::Arc;

use gpui_kit::{App, RenderImage};
use mattermost_api::Client;

use crate::resource_cache::ResourceCache;
use crate::runtime;

#[derive(Default)]
struct Inner {
    textures: HashMap<String, Arc<RenderImage>>,
    /// The head of a video file — not a decoded image, so it sits beside
    /// `textures` rather than in it. See [`Avatars::video_head`].
    heads: HashMap<String, Rc<Vec<u8>>>,
    /// In flight, so redrawing a hundred messages does not start a hundred
    /// downloads of the same face.
    pending: HashSet<String>,
    /// Fetches that failed. Retried only via [`Avatars::forget`], so a broken
    /// avatar cannot turn into a request loop driven by every redraw.
    failed: HashSet<String>,
    /// Insertion order, so the oldest can be dropped when the cache is full.
    order: Vec<String>,
    /// What the textures add up to, kept as they go in and out rather than
    /// recomputed: the answer is wanted on every insert.
    held: usize,
}

/// What a decoded texture costs in memory: four bytes a pixel, whatever it
/// was compressed to on the wire.
fn texture_bytes(texture: &RenderImage) -> usize {
    let size = texture.size(0);
    // An animation holds every one of its frames.
    (size.width.0.max(0) as usize)
        * (size.height.0.max(0) as usize)
        * 4
        * texture.frame_count().max(1)
}

impl Inner {
    /// Keeps the caches to their bounds. Called after every insert. Hands back
    /// what it dropped, because the renderer has to be told as well: it keeps
    /// whatever it uploaded until asked to let go.
    #[must_use]
    fn trim(&mut self) -> Vec<Arc<RenderImage>> {
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
    fn insert(&mut self, key: String, texture: Arc<RenderImage>) -> Vec<Arc<RenderImage>> {
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

/// Fired on the main thread when a resource lands. The key says which one, so
/// a consumer can tell a face from a file.
type LoadedCallback = Rc<RefCell<Option<Box<dyn Fn(&str, &mut App)>>>>;

/// Lets the renderer forget pictures the cache has let go of.
fn release(dropped: Vec<Arc<RenderImage>>, cx: &mut App) {
    for image in dropped {
        cx.drop_image(image, None);
    }
}

/// Marks a cache key as a file thumbnail rather than a user's picture.
const FILE_PREFIX: &str = "file:";
/// Marks a cache key as a file's larger preview image, on the same cache but
/// never colliding with its own thumbnail entry.
const FILE_PREVIEW_PREFIX: &str = "preview:";
/// Marks a cache key as an uploaded file fetched whole, because a preview of
/// an animation is a still.
const ANIMATION_PREFIX: &str = "animation:";
/// Marks a cache key as a custom emoji, looked up by name rather than by id.
const EMOJI_PREFIX: &str = "emoji:";
/// Marks a cache key as a video's head bytes (see [`Avatars::video_head`]).
const VIDEO_HEAD_PREFIX: &str = "video-head:";
/// Marks a cache key as a still taken from a video, which is made here rather
/// than fetched.
const POSTER_PREFIX: &str = "poster:";

/// How much of a video file to fetch for a poster attempt: enough to hold
/// `ftyp` + `moov` for a phone-shot clip's sample table, tiny next to the
/// clip itself. A file whose `moov` sits after `mdat` (no `-movflags
/// +faststart`) will not fit — `ui::media::head_playable` detects that
/// cheaply from what did come back, and the caller gives up rather than
/// asking for more.
const VIDEO_HEAD_BYTES: u64 = 1_500_000;

/// How much decoded picture to hold, in bytes. Counting entries was the wrong
/// unit by two orders of magnitude: an avatar is a few kilobytes and a posted
/// screenshot at draw size is four megabytes, so "a hundred and fifty of
/// them" meant anywhere between half a megabyte and six hundred. Evicting one
/// costs a refetch and nothing else — a picture still on screen is held by
/// the widget showing it.
///
/// ponytail: oldest-inserted rather than least-recently-used. An LRU needs
/// the read path to write, and the read path here is every redraw.
const MAX_TEXTURE_BYTES: usize = 96 * 1024 * 1024;

/// Video heads are a megabyte and a half each and are only read once, to make
/// a poster out of. A handful is plenty.
const MAX_HEADS: usize = 4;

/// The widest a picture is ever drawn: `message::scaled_size`'s cap, doubled
/// for a HiDPI screen. Decoding a 4000px photo to keep 500 of them is how a
/// conversation full of screenshots turns into gigabytes.
const MAX_DECODED: u32 = 1000;

/// Decodes to the size it will be drawn at rather than the size it arrived
/// in. Faces and emoji are already small; only the file images are worth
/// scaling. Runs off the main thread.
fn decode(id: &str, bytes: &[u8]) -> Result<RenderImage, image::ImageError> {
    let big = id.starts_with(FILE_PREVIEW_PREFIX)
        || id.starts_with(FILE_PREFIX)
        || id.starts_with(ANIMATION_PREFIX);
    if let Some(animation) = decode_animation(bytes, big) {
        return Ok(animation);
    }
    let mut picture = image::load_from_memory(bytes)?;
    if big && (picture.width() > MAX_DECODED || picture.height() > MAX_DECODED) {
        picture = picture.resize(
            MAX_DECODED,
            MAX_DECODED,
            image::imageops::FilterType::Triangle,
        );
    }
    Ok(render_image(picture.into_rgba8()))
}

/// How much decoded animation one picture may hold. Past this it is shown
/// as a still: a long clip saved as a GIF is hundreds of megabytes of frames.
const MAX_ANIMATION_BYTES: usize = 48 * 1024 * 1024;

/// Every frame of an animated GIF, or `None` for anything that is not one —
/// another format, a GIF with a single frame, or one too big to hold.
fn decode_animation(bytes: &[u8], big: bool) -> Option<RenderImage> {
    use image::AnimationDecoder;

    if !bytes.starts_with(b"GIF8") {
        return None;
    }
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let mut frames = Vec::new();
    let mut held = 0usize;
    for frame in decoder.into_frames() {
        // A file cut short still has the frames before the cut.
        let Ok(frame) = frame else { break };
        let delay = frame.delay();
        let mut pixels = frame.into_buffer();
        if big && (pixels.width() > MAX_DECODED || pixels.height() > MAX_DECODED) {
            pixels = image::DynamicImage::ImageRgba8(pixels)
                .resize(MAX_DECODED, MAX_DECODED, image::imageops::FilterType::Triangle)
                .into_rgba8();
        }
        held += pixels.len();
        if held > MAX_ANIMATION_BYTES {
            return None;
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        frames.push(image::Frame::from_parts(pixels, 0, 0, delay));
    }
    (frames.len() > 1).then(|| RenderImage::new(frames))
}

/// Wraps decoded pixels for the renderer, which wants them blue-first.
pub fn render_image(mut pixels: image::RgbaImage) -> RenderImage {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    RenderImage::new(vec![image::Frame::new(pixels)])
}

/// What a fetch brings back to the main thread.
enum Fetched {
    Picture(RenderImage),
    /// Raw bytes to keep, not an image to decode.
    Head(Vec<u8>),
    /// Fetched, but not a picture.
    Undecodable(String),
}

#[derive(Clone)]
pub struct Avatars {
    inner: Rc<RefCell<Inner>>,
    client: Client,
    resources: ResourceCache,
    /// Notifies the view that it should redraw. Debouncing is the caller's
    /// business.
    on_loaded: LoadedCallback,
}

impl Avatars {
    pub fn new(client: Client) -> Self {
        let resources = ResourceCache::open(client.site_url());
        Avatars {
            inner: Rc::new(RefCell::new(Inner::default())),
            client,
            resources,
            on_loaded: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn resources(&self) -> ResourceCache {
        self.resources.clone()
    }

    pub fn connect_loaded(&self, f: impl Fn(&str, &mut App) + 'static) {
        *self.on_loaded.borrow_mut() = Some(Box::new(f));
    }

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

    /// An attached image's larger preview (server-capped at 1920px wide),
    /// for showing it inline without either a blurry thumbnail or a full
    /// download of the original.
    /// An uploaded GIF, moving. The server's previews and thumbnails are
    /// stills, so this is the file itself — ask only for one small enough to
    /// be worth downloading unasked.
    pub fn file_animation(&self, file_id: &str) -> Option<Arc<RenderImage>> {
        self.cached(&format!("{ANIMATION_PREFIX}{file_id}"))
    }

    pub fn file_preview(&self, file_id: &str) -> Option<Arc<RenderImage>> {
        self.cached(&format!("{FILE_PREVIEW_PREFIX}{file_id}"))
    }

    /// The first slice of a video file's bytes — not decoded into anything,
    /// just enough for the UI layer to judge whether a poster can be built
    /// from it. `None` while the fetch is in flight or has failed; a redraw
    /// triggered by [`connect_loaded`](Avatars::connect_loaded) is what
    /// picks it up once it lands.
    ///
    /// Cached like everything else here, so scrolling a video row out of view
    /// and back does not repeat the request.
    /// A video's still, if one has already been taken. Unlike everything else
    /// here it is never fetched — it is made locally from the head of the
    /// file and handed back with [`Avatars::remember_poster`] — so a miss
    /// starts nothing.
    pub fn poster(&self, file_id: &str) -> Option<Arc<RenderImage>> {
        self.inner
            .borrow()
            .textures
            .get(&format!("{POSTER_PREFIX}{file_id}"))
            .cloned()
    }

    /// Keeps a still that was just taken, so the next redraw of that row
    /// draws a picture instead of starting a decoder over again.
    pub fn remember_poster(&self, file_id: &str, texture: Arc<RenderImage>, cx: &mut App) {
        let key = format!("{POSTER_PREFIX}{file_id}");
        let dropped = self.inner.borrow_mut().insert(key, texture);
        release(dropped, cx);
    }

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
        if let Some(texture) = self.inner.borrow().textures.get(key) {
            return Some(texture.clone());
        }
        self.request(key);
        None
    }

    /// Drops a cached picture so the next request refetches — used when a
    /// user's `last_picture_update` moves.
    pub fn forget(&self, user_id: &str, cx: &mut App) {
        {
            let mut inner = self.inner.borrow_mut();
            if let Some(texture) = inner.textures.remove(user_id) {
                inner.held = inner.held.saturating_sub(texture_bytes(&texture));
                cx.drop_image(texture, None);
            }
            inner.failed.remove(user_id);
            inner.order.retain(|key| key != user_id);
        }
        let resources = self.resources.clone();
        let key = user_id.to_string();
        runtime::runtime().spawn(async move { resources.remove(key).await });
        // Whatever is on screen still shows the old picture. Fetch at once;
        // the loaded callback redraws when the new one lands.
        self.request(user_id);
    }

    fn request(&self, user_id: &str) {
        // A post from a webhook or an integration carries no author id, and
        // the server answers a request for one with "invalid user_id" — a
        // round trip to be told what is already known here.
        if user_id.is_empty() || user_id.ends_with(':') {
            return;
        }
        {
            let mut inner = self.inner.borrow_mut();
            if inner.pending.contains(user_id) || inner.failed.contains(user_id) {
                return;
            }
            inner.pending.insert(user_id.to_string());
        }

        let client = self.client.clone();
        let resources = self.resources.clone();
        let fetch_id = user_id.to_string();
        let done_id = user_id.to_string();
        let this = self.clone();

        runtime::spawn(
            async move {
                let ttl = if fetch_id.starts_with(EMOJI_PREFIX)
                    || (!fetch_id.contains(':') && !fetch_id.is_empty())
                {
                    // Names and profile pictures are mutable. A websocket
                    // update invalidates them immediately while the app is
                    // open; this bounds staleness across restarts.
                    std::time::Duration::from_secs(24 * 60 * 60)
                } else {
                    // File ids identify immutable uploads.
                    std::time::Duration::from_secs(30 * 24 * 60 * 60)
                };
                let cache_key = fetch_id.clone();
                let key = fetch_id.clone();
                let bytes = resources
                    .get_or_fetch(cache_key, ttl, || async move {
                        if let Some(file_id) = fetch_id.strip_prefix(FILE_PREFIX) {
                            return client.file_thumbnail_bytes(file_id).await;
                        }
                        if let Some(file_id) = fetch_id.strip_prefix(FILE_PREVIEW_PREFIX) {
                            return client.file_preview_bytes(file_id).await;
                        }
                        if let Some(file_id) = fetch_id.strip_prefix(ANIMATION_PREFIX) {
                            return client.download_file(file_id).await;
                        }
                        if let Some(file_id) = fetch_id.strip_prefix(VIDEO_HEAD_PREFIX) {
                            return client.download_file_range(file_id, VIDEO_HEAD_BYTES).await;
                        }
                        if let Some(name) = fetch_id.strip_prefix(EMOJI_PREFIX) {
                            let emoji = client.emoji_by_name(name).await?;
                            return client.emoji_image_bytes(&emoji.id).await;
                        }
                        client.user_image_bytes(&fetch_id).await
                    })
                    .await?;
                if key.starts_with(VIDEO_HEAD_PREFIX) {
                    return Ok(Fetched::Head(bytes));
                }
                // Decoding is the expensive half, and a screenful of faces
                // arriving together is exactly when the frame cannot spare it.
                let decoded = tokio::task::spawn_blocking(move || {
                    decode(&key, &bytes).map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
                Ok::<_, mattermost_api::Error>(match decoded {
                    Ok(picture) => Fetched::Picture(picture),
                    Err(error) => Fetched::Undecodable(error),
                })
            },
            move |result, cx| {
                let mut loaded = false;
                let mut dropped = Vec::new();
                {
                    let mut inner = this.inner.borrow_mut();
                    inner.pending.remove(&done_id);
                    match result {
                        // Raw bytes to keep, not an image to decode — the
                        // caller inspects and plays these, this cache just
                        // spares it a second fetch on the next redraw.
                        Ok(Fetched::Head(bytes)) => {
                            inner.heads.insert(done_id.clone(), Rc::new(bytes));
                            dropped = inner.trim();
                            loaded = true;
                        }
                        Ok(Fetched::Picture(picture)) => {
                            dropped = inner.insert(done_id.clone(), Arc::new(picture));
                            loaded = true;
                        }
                        // A shortcode that is neither Unicode nor uploaded to
                        // this server is an ordinary thing for someone to
                        // type, not a failure worth the word "failed".
                        Ok(Fetched::Undecodable(_)) | Err(_)
                            if done_id.starts_with(EMOJI_PREFIX) =>
                        {
                            tracing::debug!(
                                emoji = done_id.trim_start_matches(EMOJI_PREFIX),
                                "no such custom emoji on this server"
                            );
                            inner.failed.insert(done_id.clone());
                        }
                        Ok(Fetched::Undecodable(error)) => {
                            tracing::warn!(%error, "undecodable avatar image");
                            inner.failed.insert(done_id.clone());
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, "avatar fetch failed");
                            inner.failed.insert(done_id.clone());
                        }
                    }
                }
                release(dropped, cx);
                // The borrow is released before the callback: it will almost
                // certainly redraw, and redrawing reads this same map.
                if loaded {
                    if let Some(callback) = this.on_loaded.borrow().as_ref() {
                        callback(&done_id, cx);
                    }
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, texture_bytes, MAX_DECODED};

    fn gif(frames: usize, side: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for index in 0..frames {
                let pixels = image::RgbaImage::from_pixel(
                    side,
                    side,
                    image::Rgba([index as u8 * 40, 0, 0, 255]),
                );
                let delay = image::Delay::from_numer_denom_ms(100, 1);
                encoder
                    .encode_frame(image::Frame::from_parts(pixels, 0, 0, delay))
                    .unwrap();
            }
        }
        bytes
    }

    #[test]
    fn an_animated_gif_keeps_every_frame() {
        let animation = decode("emoji:party", &gif(3, 8)).unwrap();
        assert_eq!(animation.frame_count(), 3);
        // And is counted as three pictures' worth of memory, not one.
        assert_eq!(texture_bytes(&animation), 8 * 8 * 4 * 3);
    }

    #[test]
    fn a_single_frame_gif_is_a_still() {
        assert_eq!(decode("emoji:still", &gif(1, 8)).unwrap().frame_count(), 1);
    }

    #[test]
    fn a_big_attachment_animation_is_scaled_down_frame_by_frame() {
        let animation = decode("animation:file", &gif(2, MAX_DECODED + 200)).unwrap();
        assert_eq!(animation.frame_count(), 2);
        assert_eq!(animation.size(1).width.0 as u32, MAX_DECODED);
    }
}
