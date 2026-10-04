use gpui_kit::component::select::SearchableVec;
use gpui_kit::component::ThemeMode;
use gpui_kit::{Context, Window};

use super::dialog::Settings;
use crate::zed_extensions::{self, Extension};
use crate::{appearance, runtime, themes};

impl Settings {
    pub(super) fn install(&mut self, extension: &Extension, cx: &mut Context<Self>) {
        let id = extension.id.clone();
        let name = extension.name.clone();
        let work = {
            let id = id.clone();
            async move {
                let files = zed_extensions::download(&id).await.map_err(|e| e.to_string())?;
                themes::install(&id, &files).map_err(|e| e.to_string())
            }
        };
        self.change_themes(id, work, cx, move |added| match added {
            1 => format!("{name}: one theme added to the lists above."),
            added => format!("{name}: {added} themes added to the lists above."),
        });
    }

    pub(super) fn remove(&mut self, extension: &Extension, cx: &mut Context<Self>) {
        let id = extension.id.clone();
        let name = extension.name.clone();
        let work = {
            let id = id.clone();
            async move { themes::remove(&id).map(|()| 0).map_err(|e| e.to_string()) }
        };
        self.change_themes(id, work, cx, move |_| format!("{name} was removed."));
    }

    /// Runs an install or a removal off the main thread and then makes the
    /// window agree with the directory, whichever way it went.
    fn change_themes(
        &mut self,
        id: String,
        work: impl Future<Output = Result<usize, String>> + Send + 'static,
        cx: &mut Context<Self>,
        done: impl FnOnce(usize) -> String + 'static,
    ) {
        self.busy.insert(id.clone());
        self.notice = None;
        cx.notify();
        let view = cx.entity().downgrade();
        let ui = self.ui.clone();
        runtime::spawn(work, move |result, cx| {
            // The session outlives its window: the themes are read again
            // even if the dialog this started from is gone.
            themes::load(cx);
            let notice = match result {
                Ok(count) => done(count),
                Err(error) => error,
            };
            ui.with_window(cx, |window, cx| {
                appearance::apply(Some(window), cx);
                let _ = view.update(cx, |settings, cx| {
                    settings.busy.remove(&id);
                    settings.notice = Some(notice.into());
                    settings.themes_changed(window, cx);
                });
            });
        });
    }

    /// Makes the two theme lists say what there is to choose from now.
    fn themes_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.installed = themes::installed();
        for (state, mode) in [(&self.light, ThemeMode::Light), (&self.dark, ThemeMode::Dark)] {
            let names = themes::names(mode, cx);
            let worn = appearance::theme_for(mode, cx);
            state.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(names), window, cx);
                state.set_selected_value(&worn, window, cx);
            });
        }
        cx.notify();
    }
}
