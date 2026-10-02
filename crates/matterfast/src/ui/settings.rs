//! The settings dialog: how the window looks.
//!
//! Every choice here takes effect as it is made — there is no Save, because
//! the only way to judge a theme or a font is to see it, and a dialog that
//! makes you commit first and look afterwards has that backwards.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonGroup};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, IndexPath, Selectable, Sizable, WindowExt,
};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, App, Context, Entity, SharedString, Subscription, Window};

use super::Ui;
use crate::appearance::{self, FontRole, ThemeChoice};

type Fonts = SelectState<SearchableVec<SharedString>>;

struct Settings {
    interface: Entity<Fonts>,
    code: Entity<Fonts>,
    _subscriptions: Vec<Subscription>,
}

/// What the first entry of a font list is called: the font the application
/// starts with, by name, so "the default" is not a mystery.
fn built_in_label(family: &SharedString) -> SharedString {
    format!("{family} (default)").into()
}

impl Settings {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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

        Settings {
            interface,
            code,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let row = |title: &'static str, subtitle: &'static str| {
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().child(title))
                .child(div().text_xs().text_color(muted).child(subtitle))
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
                    .child(row("Interface font", "Messages, lists and everything else"))
                    .child(
                        div().w(px(240.)).child(
                            Select::new(&self.interface)
                                .small()
                                .search_placeholder("Search fonts"),
                        ),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(row("Code font", "Inline code and code blocks"))
                    .child(
                        div().w(px(240.)).child(
                            Select::new(&self.code)
                                .small()
                                .search_placeholder("Search fonts"),
                        ),
                    ),
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
    }
}

pub fn show(ui: &Rc<Ui>, cx: &mut App) {
    ui.with_window(cx, move |window, cx| {
        let view = cx.new(|cx| Settings::new(window, cx));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title("Settings").w(px(520.)).child(view.clone())
        });
    });
}
