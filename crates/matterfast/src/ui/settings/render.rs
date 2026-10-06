use gpui_kit::component::button::{Button, ButtonGroup};
use gpui_kit::component::select::Select;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Selectable, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Context, Entity, Window};

use super::dialog::Settings;
use super::names::Names;
use crate::appearance::{self, ThemeChoice};

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
                                settings.browsing = !settings.browsing;
                                cx.notify();
                                if settings.browsing {
                                    // The first opening asks for the list,
                                    // and so does one after a failure.
                                    settings.browser.update(cx, |browser, cx| {
                                        browser.load(cx);
                                        browser.focus(window, cx);
                                    });
                                }
                            })),
                    ),
            )
            .when(self.browsing, |column| column.child(self.browser.clone()))
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
