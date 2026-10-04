use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::App;
use mattermost_api::Client;

use super::inner::Inner;
use super::service::Avatars;
use crate::resource_cache::ResourceCache;

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
}
