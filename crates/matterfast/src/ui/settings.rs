//! The settings dialog: how the window looks.
//!
//! Every choice here takes effect as it is made — there is no Save, because
//! the only way to judge a theme or a font is to see it, and a dialog that
//! makes you commit first and look afterwards has that backwards.
//!
//! Themes are chosen twice, one for light and one for dark, so that "System"
//! has a pair to switch between. More of them come from Zed's extension
//! registry, which this dialog can browse: the list is asked for when the
//! section is first opened and searched locally after that, so typing costs
//! no requests. The registry is not a Mattermost server and needs no session,
//! which is why its two calls are made from here ([`crate::zed_extensions`])
//! rather than from `ui/mod.rs` with the rest.

use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonGroup};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, IndexPath, Selectable, Sizable, ThemeMode, WindowExt,
};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, AnyElement, App, Context, ElementId, Entity, SharedString, Subscription, Window};

use super::kit::Lucide;
use super::Ui;
use crate::appearance::{self, FontRole, ThemeChoice};
use crate::zed_extensions::{self, Extension};
use crate::{runtime, themes};

type Names = SelectState<SearchableVec<SharedString>>;

/// How many registry entries are drawn at once. The list is not virtual, and
/// nobody reads seven hundred rows: past this, the search box is the way in.
const SHOWN: usize = 40;

/// What is known of the registry's list.
enum Registry {
    NotAsked,
    Loading,
    Failed(String),
    Loaded(Vec<Extension>),
}

struct Settings {
    ui: Rc<Ui>,
    light: Entity<Names>,
    dark: Entity<Names>,
    interface: Entity<Names>,
    code: Entity<Names>,
    /// Whether the registry section is open.
    browsing: bool,
    search: Entity<InputState>,
    registry: Registry,
    installed: BTreeSet<String>,
    /// Extensions being installed or removed right now.
    busy: HashSet<String>,
    /// What the last install or removal had to say.
    notice: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

/// What the first entry of a font list is called: the font the application
/// starts with, by name, so "the default" is not a mystery.
fn built_in_label(family: &SharedString) -> SharedString {
    format!("{family} (default)").into()
}

/// A download count the way a list row has room for it.
fn downloads(count: u64) -> String {
    match count {
        0..1_000 => count.to_string(),
        1_000..1_000_000 => format!("{}K", count / 1_000),
        _ => format!("{:.1}M", count as f64 / 1_000_000.0),
    }
}

/// The extensions a search leaves, in the registry's order, and how many
/// there were before the list was cut to what is drawn.
fn found<'a>(extensions: &'a [Extension], query: &str) -> (Vec<&'a Extension>, usize) {
    let needle = query.trim().to_lowercase();
    let matching: Vec<&Extension> = extensions
        .iter()
        .filter(|extension| extension.matches(&needle))
        .collect();
    let total = matching.len();
    (matching.into_iter().take(SHOWN).collect(), total)
}

impl Settings {
    fn new(ui: Rc<Ui>, window: &mut Window, cx: &mut Context<Self>) -> Self {
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

        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search Zed themes"));
        subscriptions.push(cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        }));

        Settings {
            ui,
            light,
            dark,
            interface,
            code,
            browsing: false,
            search,
            registry: Registry::NotAsked,
            installed: themes::installed(),
            busy: HashSet::new(),
            notice: None,
            _subscriptions: subscriptions,
        }
    }

    /// Opens or closes the registry section; the first opening asks for the
    /// list, and so does one after a failure.
    fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn install(&mut self, extension: &Extension, cx: &mut Context<Self>) {
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

    fn remove(&mut self, extension: &Extension, cx: &mut Context<Self>) {
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

    fn registry_rows(&self, cx: &mut Context<Self>) -> AnyElement {
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

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let row = |title: &'static str, subtitle: &'static str| {
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().child(title))
                .child(div().text_xs().text_color(muted).child(subtitle))
        };
        let select = |state: &Entity<Names>, placeholder: &'static str| {
            div()
                .w(px(240.))
                .child(Select::new(state).small().search_placeholder(placeholder))
        };
        let current = appearance::theme_choice();

        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(row("Theme", "System follows the desktop as it changes"))
                    .child(
                        ButtonGroup::new("theme")
                            .outline()
                            .small()
                            .children(ThemeChoice::ALL.map(|choice| {
                                Button::new(choice.label())
                                    .label(choice.label())
                                    .selected(choice == current)
                            }))
                            .on_click(|picked, window, cx| {
                                if let Some(choice) =
                                    picked.first().and_then(|index| ThemeChoice::ALL.get(*index))
                                {
                                    appearance::set_theme_choice(*choice, window, cx);
                                }
                            }),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(row("Light theme", "Worn when the theme is light"))
                    .child(select(&self.light, "Search themes")),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(row("Dark theme", "Worn when the theme is dark"))
                    .child(select(&self.dark, "Search themes")),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(row("More themes", "From the Zed editor's extension registry"))
                    .child(
                        Button::new("browse-themes")
                            .small()
                            .outline()
                            .label(if self.browsing { "Done" } else { "Browse\u{2026}" })
                            .on_click(cx.listener(|settings, _, window, cx| {
                                settings.browse(window, cx);
                            })),
                    ),
            )
            .when(self.browsing, |column| {
                column
                    .child(Input::new(&self.search).prefix(Lucide::Search).cleanable(true))
                    .child(
                        div()
                            .id("zed-themes")
                            .h(px(220.))
                            .border_1()
                            .border_color(border)
                            .rounded(cx.theme().radius)
                            .overflow_y_scroll()
                            .child(self.registry_rows(cx)),
                    )
                    .children(
                        self.notice
                            .clone()
                            .map(|notice| div().text_xs().text_color(muted).child(notice)),
                    )
            })
            // The registry takes the room the fonts had rather than adding to
            // it: the dialog is as tall as a small window already.
            .when(!self.browsing, |column| {
                column
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(row("Interface font", "Messages, lists and everything else"))
                        .child(select(&self.interface, "Search fonts")),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(row("Code font", "Inline code and code blocks"))
                        .child(select(&self.code, "Search fonts")),
                )
                .child(div().text_xs().text_color(muted).child(
                    "A variable font is loaded as its regular face only, so bold text \
                     will not look bold in one. The default interface font does not have this problem.",
                ))
                // CC-BY asks for the credit to be somewhere a person using the
                // application can see it, not only in the source tree.
                .child(div().text_xs().text_color(muted).child(
                    "Fonts: Inter \u{a9} The Inter Project Authors, SIL OFL 1.1. Emoji: Twemoji \
                     \u{a9} Twitter, Inc and other contributors, CC-BY 4.0, in the Twemoji Mozilla \
                     font \u{a9} Mozilla Foundation, Apache 2.0.",
                ))
            })
    }
}

pub fn show(ui: &Rc<Ui>, cx: &mut App) {
    let session = ui.clone();
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| Settings::new(session, window, cx));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title("Settings").w(px(520.)).child(view.clone())
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extension(id: &str, name: &str) -> Extension {
        Extension {
            id: id.into(),
            name: name.into(),
            description: None,
            authors: vec![],
            download_count: 0,
        }
    }

    #[test]
    fn a_download_count_is_short_enough_for_a_row() {
        assert_eq!(downloads(0), "0");
        assert_eq!(downloads(812), "812");
        assert_eq!(downloads(1_000), "1K");
        assert_eq!(downloads(496_067), "496K");
        assert_eq!(downloads(1_163_124), "1.2M");
    }

    #[test]
    fn a_search_keeps_the_registry_order_and_ignores_case_and_edges() {
        let extensions = [
            extension("catppuccin", "Catppuccin"),
            extension("tokyo-night", "Tokyo Night"),
            extension("catppuccin-blur", "Catppuccin Blur"),
        ];
        let (shown, total) = found(&extensions, "  CATP ");
        assert_eq!(total, 2);
        let ids: Vec<_> = shown.iter().map(|extension| extension.id.as_str()).collect();
        assert_eq!(ids, ["catppuccin", "catppuccin-blur"]);
        assert_eq!(found(&extensions, "").1, 3);
    }

    #[test]
    fn a_long_list_is_cut_and_says_how_long_it_was() {
        let extensions: Vec<Extension> = (0..SHOWN + 25)
            .map(|index| extension(&format!("theme-{index}"), "Theme"))
            .collect();
        let (shown, total) = found(&extensions, "theme");
        assert_eq!(shown.len(), SHOWN);
        assert_eq!(total, SHOWN + 25);
    }
}
