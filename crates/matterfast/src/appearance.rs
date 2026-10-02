//! How the window looks: light or dark, and in which fonts.
//!
//! Three choices, kept in the same `settings.json` as everything else that is
//! about this machine rather than about an account. "System" is the default
//! and means what the desktop says right now — and again whenever it changes
//! its mind, which a desktop on a day/night schedule does twice a day.
//!
//! The fonts default to the ones the application brings with it (see
//! [`crate::fonts`] for why it has to bring any). Choosing a system font is
//! the person's call to make, with one honest caveat the settings dialog
//! repeats: a variable font is loaded as its regular face only, so nothing in
//! it comes out bold.

use std::sync::OnceLock;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, SharedString, Window};

use crate::background::{set_setting, setting};

/// Light, dark, or whatever the desktop is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "System",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
        }
    }

    fn stored(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    /// Anything unrecognised is the default: a settings file from a later
    /// version must not leave this one without a theme.
    fn parse(text: &str) -> ThemeChoice {
        match text {
            "light" => ThemeChoice::Light,
            "dark" => ThemeChoice::Dark,
            _ => ThemeChoice::System,
        }
    }
}

/// Which of the two fonts a choice is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontRole {
    /// Everything that is not code.
    Interface,
    /// Inline code and code blocks.
    Code,
}

impl FontRole {
    fn key(self) -> &'static str {
        match self {
            FontRole::Interface => "ui_font",
            FontRole::Code => "code_font",
        }
    }
}

pub fn theme_choice() -> ThemeChoice {
    setting("theme")
        .and_then(|value| value.as_str().map(ThemeChoice::parse))
        .unwrap_or_default()
}

/// The family chosen for a role, or nothing for the built-in one.
pub fn font_choice(role: FontRole) -> Option<String> {
    setting(role.key())
        .and_then(|value| value.as_str().map(str::to_string))
        .filter(|family| !family.is_empty())
}

pub fn set_theme_choice(choice: ThemeChoice, window: &mut Window, cx: &mut App) {
    set_setting("theme", choice.stored().into());
    apply(Some(window), cx);
}

/// `None` goes back to the built-in font.
pub fn set_font_choice(role: FontRole, family: Option<String>, window: &mut Window, cx: &mut App) {
    set_setting(role.key(), family.unwrap_or_default().into());
    apply(Some(window), cx);
}

/// The fonts in force before anyone chose anything: (interface, code).
static BUILT_IN: OnceLock<(SharedString, SharedString)> = OnceLock::new();

/// The built-in families, as (interface, code).
pub fn built_in_fonts(cx: &App) -> (SharedString, SharedString) {
    BUILT_IN
        .get_or_init(|| {
            let theme = Theme::global(cx);
            (theme.font_family.clone(), theme.mono_font_family.clone())
        })
        .clone()
}

/// Makes the theme say what the settings say. Call after the bundled fonts
/// are installed, again once there is a window — on Linux only a window knows
/// what the desktop's appearance is — and whenever a choice changes.
pub fn apply(window: Option<&mut Window>, cx: &mut App) {
    let built_in = built_in_fonts(cx);

    let mode = match theme_choice() {
        ThemeChoice::Light => ThemeMode::Light,
        ThemeChoice::Dark => ThemeMode::Dark,
        ThemeChoice::System => window
            .as_ref()
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance())
            .into(),
    };
    // Loading a mode loads its theme file, fonts and all, so the fonts go on
    // afterwards and in an update of their own.
    if Theme::global(cx).mode != mode {
        Theme::change(mode, None, cx);
    }

    let known = cx.text_system().all_font_names();
    // A font that was uninstalled since it was chosen is not a font; the
    // setting stays, in case it comes back, and the built-in one is used.
    let pick = |role: FontRole, fallback: &SharedString| -> SharedString {
        font_choice(role)
            .filter(|family| known.iter().any(|name| name == family))
            .map(SharedString::from)
            .unwrap_or_else(|| fallback.clone())
    };
    let (interface, code) = (
        pick(FontRole::Interface, &built_in.0),
        pick(FontRole::Code, &built_in.1),
    );
    let theme = Theme::global(cx);
    if theme.font_family != interface || theme.mono_font_family != code {
        Theme::update(cx, |theme| {
            theme.font_family = interface;
            theme.mono_font_family = code;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_choice_survives_being_stored() {
        for choice in ThemeChoice::ALL {
            assert_eq!(ThemeChoice::parse(choice.stored()), choice);
        }
    }

    #[test]
    fn an_unknown_theme_is_the_system_one() {
        assert_eq!(ThemeChoice::parse("sepia"), ThemeChoice::System);
        assert_eq!(ThemeChoice::parse(""), ThemeChoice::System);
    }
}
