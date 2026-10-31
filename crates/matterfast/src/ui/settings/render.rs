use gpui_adaptive::{Choice, Command, Fields, Segments, Setting, Text};
use gpui_kit::prelude::*;
use gpui_kit::{rems, Context, Entity, Window};

use super::dialog::Settings;
use super::names::Names;
use crate::appearance::{self, ThemeChoice};

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let choice = |id: &'static str, state: &Entity<Names>| {
            gpui_kit::div().w(rems(15.)).child(Choice::new(state).id(id))
        };
        let current = appearance::theme_choice();
        let theme = ThemeChoice::ALL
            .iter()
            .fold(Segments::new("theme", current.label()), |segments, choice| {
                segments.segment(choice.label(), choice.label())
            })
            .on_change(|picked, window, cx| {
                if let Some(choice) = ThemeChoice::ALL.iter().find(|choice| choice.label() == picked.as_ref()) {
                    appearance::set_theme_choice(*choice, window, cx);
                }
            });
        let settings = cx.weak_entity();
        let browse = Command::new("browse-themes", if self.browsing { "Done" } else { "Browse\u{2026}" })
            .on_trigger(move |window, cx| {
                settings
                    .update(cx, |settings, cx| {
                        settings.browsing = !settings.browsing;
                        cx.notify();
                        if settings.browsing {
                            // The first opening asks for the list, and so does
                            // one after a failure.
                            settings.browser.update(cx, |browser, cx| {
                                browser.load(cx);
                                browser.focus(window, cx);
                            });
                        }
                    })
                    .ok();
            })
            .as_button();

        Fields::new()
            .child(Setting::new("Theme").detail("System follows the desktop as it changes").control(theme))
            .child(Setting::new("Light theme").detail("Worn when the theme is light").control(choice("light", &self.light)))
            .child(Setting::new("Dark theme").detail("Worn when the theme is dark").control(choice("dark", &self.dark)))
            .child(Setting::new("More themes").detail("From the Zed editor's extension registry").control(browse))
            .when(self.browsing, |fields| fields.child(self.browser.clone()))
            // The registry takes the room the fonts had rather than adding to
            // it: the dialog is as tall as a small window already.
            .when(!self.browsing, |fields| {
                fields
                    .child(
                        Setting::new("Interface font")
                            .detail("Messages, lists and everything else")
                            .control(choice("interface-font", &self.interface)),
                    )
                    .child(
                        Setting::new("Code font")
                            .detail("Inline code and code blocks")
                            .control(choice("code-font", &self.code)),
                    )
                    .child(Text::small(
                        "A variable font is loaded as its regular face only, so bold text \
                         will not look bold in one. The default interface font does not have this problem.",
                    )
                    .muted())
                    // CC-BY asks for the credit to be somewhere a person using
                    // the application can see it, not only in the source tree.
                    .child(Text::small(
                        "Fonts: Inter \u{a9} The Inter Project Authors, SIL OFL 1.1. Emoji: Twemoji \
                         \u{a9} Twitter, Inc and other contributors, CC-BY 4.0, in the Twemoji Mozilla \
                         font \u{a9} Mozilla Foundation, Apache 2.0.",
                    )
                    .muted())
            })
    }
}
