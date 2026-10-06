use gpui_kit::component::select::{SearchableVec, SelectEvent, SelectState};
use gpui_kit::component::{IndexPath, ThemeMode};
use gpui_kit::prelude::*;
use gpui_kit::{Context, SharedString, Window};
use gpui_zed_themes::{Browser, BrowserEvent};

use super::built_in_label::built_in_label;
use super::dialog::Settings;
use crate::appearance::{self, FontRole};
use crate::themes;

impl Settings {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (interface_default, code_default) = appearance::built_in_fonts(cx);
        let families: Vec<SharedString> = cx
            .text_system()
            .all_font_names()
            .into_iter()
            .map(SharedString::from)
            .collect();

        let mut subscriptions = Vec::new();
        let mut picker = |role: FontRole, default: &SharedString| {
            let built_in = built_in_label(default);
            let mut items = vec![built_in.clone()];
            items.extend(families.iter().cloned());
            // A chosen font that is no longer installed reads as the built-in
            // one, which is also what is being drawn.
            let selected = appearance::font_choice(role)
                .and_then(|chosen| families.iter().position(|family| *family == chosen))
                .map_or(0, |position| position + 1);
            let state = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(items),
                    Some(IndexPath::default().row(selected)),
                    window,
                    cx,
                )
                .searchable(true)
            });
            subscriptions.push(cx.subscribe_in(
                &state,
                window,
                move |_, _, event: &SelectEvent<SearchableVec<SharedString>>, window, cx| {
                    let SelectEvent::Confirm(Some(family)) = event else {
                        return;
                    };
                    let family = (*family != built_in).then(|| family.to_string());
                    appearance::set_font_choice(role, family, window, cx);
                },
            ));
            state
        };
        let interface = picker(FontRole::Interface, &interface_default);
        let code = picker(FontRole::Code, &code_default);

        let mut theme_picker = |mode: ThemeMode| {
            let names = themes::names(mode, cx);
            // A chosen theme that is no longer there reads as the one being
            // worn in its place.
            let worn = appearance::theme_for(mode, cx);
            let selected = names.iter().position(|name| *name == worn).unwrap_or(0);
            let state = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(names),
                    Some(IndexPath::default().row(selected)),
                    window,
                    cx,
                )
                .searchable(true)
            });
            subscriptions.push(cx.subscribe_in(
                &state,
                window,
                move |_, _, event: &SelectEvent<SearchableVec<SharedString>>, window, cx| {
                    if let SelectEvent::Confirm(Some(name)) = event {
                        appearance::set_theme_name(mode, name, window, cx);
                    }
                },
            ));
            state
        };
        let light = theme_picker(ThemeMode::Light);
        let dark = theme_picker(ThemeMode::Dark);

        let browser =
            cx.new(|cx| Browser::new(themes::registry(), themes::store(), window, cx));
        subscriptions.push(cx.subscribe_in(
            &browser,
            window,
            |settings, _, _: &BrowserEvent, window, cx| settings.themes_changed(window, cx),
        ));

        Settings {
            light,
            dark,
            interface,
            code,
            browsing: false,
            browser,
            _subscriptions: subscriptions,
        }
    }
}
