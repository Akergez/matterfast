use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::App;

/// Fired on the main thread when a resource lands. The key says which one, so
/// a consumer can tell a face from a file.
pub(super) type LoadedCallback = Rc<RefCell<Option<Box<dyn Fn(&str, &mut App)>>>>;
