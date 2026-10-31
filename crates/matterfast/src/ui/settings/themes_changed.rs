use gpui_adaptive::theme::ThemeMode;
use gpui_kit::{Context, Window};

use super::dialog::Settings;
use super::names::options;
use crate::{appearance, themes};

impl Settings {
    /// An extension was installed or removed: reads the themes again, makes
    /// the window wear what the settings say out of what there is now — a
    /// theme that was being worn may be gone — and makes the two lists say
    /// what there is to choose from.
    pub(super) fn themes_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        themes::load(cx);
        appearance::apply(Some(window), cx);
        for (state, mode) in [(&self.light, ThemeMode::Light), (&self.dark, ThemeMode::Dark)] {
            let names = themes::names(mode, cx);
            let worn = appearance::theme_for(mode, cx);
            state.update(cx, |state, cx| state.set_options(options(names), Some(worn), window, cx));
        }
        cx.notify();
    }
}
