use std::rc::Rc;
use std::sync::Arc;

use super::constants::{
    ANIMATION_PREFIX, EMOJI_PREFIX, FILE_PREFIX, FILE_PREVIEW_PREFIX, VIDEO_HEAD_BYTES,
    VIDEO_HEAD_PREFIX,
};
use super::decode::decode;
use super::fetched::Fetched;
use super::release::release;
use super::service::Avatars;
use crate::runtime;

impl Avatars {
    pub(super) fn request(&self, user_id: &str) {
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
