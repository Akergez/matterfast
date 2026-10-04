use std::cell::RefCell;
use std::rc::Rc;

use mattermost_api::Client;

use super::inner::Inner;
use super::loaded_callback::LoadedCallback;
use crate::resource_cache::ResourceCache;

#[derive(Clone)]
pub struct Avatars {
    pub(super) inner: Rc<RefCell<Inner>>,
    pub(super) client: Client,
    pub(super) resources: ResourceCache,
    /// Notifies the view that it should redraw. Debouncing is the caller's
    /// business.
    pub(super) on_loaded: LoadedCallback,
}
