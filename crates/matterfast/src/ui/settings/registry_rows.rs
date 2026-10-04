use gpui_kit::component::button::Button;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, AnyElement, Context, ElementId, SharedString};

use super::dialog::Settings;
use super::downloads::downloads;
use super::found::found;
use super::registry::Registry;

impl Settings {
    pub(super) fn registry_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let note = |text: SharedString| {
            div().p_3().text_sm().text_color(muted).child(text).into_any_element()
        };
        let extensions = match &self.registry {
            Registry::NotAsked | Registry::Loading => {
                return h_flex()
                    .p_3()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .text_color(muted)
                    .child(Spinner::new().small())
                    .child("Asking Zed's registry\u{2026}")
                    .into_any_element();
            }
            Registry::Failed(error) => {
                return note(format!("The registry did not answer: {error}").into());
            }
            Registry::Loaded(extensions) => extensions,
        };
        let query = self.search.read(cx).value();
        let (shown, total) = found(extensions, &query);
        if shown.is_empty() {
            return note("No theme extension matches that.".into());
        }

        let mut rows = v_flex().gap_1();
        for extension in &shown {
            let id = &extension.id;
            let installed = self.installed.contains(id);
            let mut by = extension.author().map(str::to_string).unwrap_or_default();
            if !by.is_empty() {
                by.push_str(" \u{b7} ");
            }
            by.push_str(&format!("{} downloads", downloads(extension.download_count)));
            let title = if extension.name.is_empty() { id.clone() } else { extension.name.clone() };

            let button = Button::new(ElementId::Name(format!("zed-theme-{id}").into()))
                .small()
                .outline()
                .loading(self.busy.contains(id));
            let subject = (*extension).clone();
            let button = if installed {
                button.label("Remove").on_click(cx.listener(move |settings, _, _, cx| {
                    settings.remove(&subject, cx);
                }))
            } else {
                button.label("Install").on_click(cx.listener(move |settings, _, _, cx| {
                    settings.install(&subject, cx);
                }))
            };
            rows = rows.child(
                h_flex()
                    .gap_3()
                    .px_2()
                    .py_1p5()
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(title))
                            .children(extension.description.clone().map(|description| {
                                div().text_xs().text_color(muted).truncate().child(description)
                            }))
                            .child(div().text_xs().text_color(muted).truncate().child(by)),
                    )
                    .child(button),
            );
        }
        if total > shown.len() {
            rows = rows.child(note(
                format!("{} of {total} shown. Search to find the rest.", shown.len()).into(),
            ));
        }
        rows.into_any_element()
    }
}
