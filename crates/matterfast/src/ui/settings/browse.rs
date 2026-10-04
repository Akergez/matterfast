use gpui_kit::{Context, Window};

use super::dialog::Settings;
use super::registry::Registry;
use crate::{runtime, zed_extensions};

impl Settings {
    /// Opens or closes the registry section; the first opening asks for the
    /// list, and so does one after a failure.
    pub(super) fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browsing = !self.browsing;
        cx.notify();
        if !self.browsing {
            return;
        }
        self.search.update(cx, |search, cx| search.focus(window, cx));
        if !matches!(self.registry, Registry::NotAsked | Registry::Failed(_)) {
            return;
        }
        self.registry = Registry::Loading;
        let view = cx.entity().downgrade();
        runtime::spawn(zed_extensions::list(), move |listed, cx| {
            let _ = view.update(cx, |settings, cx| {
                settings.registry = match listed {
                    Ok(extensions) => Registry::Loaded(extensions),
                    Err(error) => Registry::Failed(error.to_string()),
                };
                cx.notify();
            });
        });
    }
}
