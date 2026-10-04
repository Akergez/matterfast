use std::rc::Rc;

use gpui_kit::App;

/// Something to run once the click that asked for it is over.
pub(crate) type Callback = Rc<dyn Fn(&mut App)>;

pub(crate) fn run_later(callback: &Callback, cx: &mut App) {
    let callback = callback.clone();
    cx.defer(move |cx| callback(cx));
}
