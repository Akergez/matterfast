use gpui_kit::App;

use super::service::Avatars;
use super::texture_bytes::texture_bytes;
use crate::runtime;

impl Avatars {
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
}
