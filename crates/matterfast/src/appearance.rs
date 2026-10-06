//! How the window looks: light or dark, and in which fonts.
//!
//! Three choices, kept in the same `settings.json` as everything else that is
//! about this machine rather than about an account. "System" is the default
//! and means what the desktop says right now — and again whenever it changes
//! its mind, which a desktop on a day/night schedule does twice a day.
//!
//! Which theme is worn is a second choice, made once for each of the two:
//! a light theme and a dark one, by name, out of [`crate::themes`]. Two
//! choices rather than one list are what lets "System" go on meaning
//! something when the themes are not the built-in pair.
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
use crate::themes;

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

/// The setting a mode's theme is kept under.
fn theme_key(mode: ThemeMode) -> &'static str {
    if mode.is_dark() { "dark_theme" } else { "light_theme" }
}

/// The theme chosen for a mode, or nothing for the built-in one. It is a
/// name and may be the name of a theme that is no longer there.
pub fn theme_name(mode: ThemeMode) -> Option<String> {
    setting(theme_key(mode))
        .and_then(|value| value.as_str().map(str::to_string))
        .filter(|name| !name.is_empty())
}

/// The name of what a mode wears: the chosen theme, or the scheme when
/// nothing was chosen or the choice is gone.
pub fn theme_for(mode: ThemeMode, cx: &App) -> SharedString {
    themes::pick(mode, theme_name(mode).as_deref(), cx)
        .map_or(themes::SCHEME.into(), |theme| theme.name.clone())
}

pub fn set_theme_name(mode: ThemeMode, name: &str, window: &mut Window, cx: &mut App) {
    set_setting(theme_key(mode), name.into());
    apply(Some(window), cx);
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

/// Whether the phone has changed between light and dark since the theme was
/// last made to follow it.
#[cfg(target_os = "android")]
pub fn behind_the_system(cx: &App) -> bool {
    matches!(theme_choice(), ThemeChoice::System)
        && gpui_mobile::android::jni::query_night_mode_via_jni() != Theme::global(cx).mode.is_dark()
}

/// Makes the theme say what the settings say. Call after the bundled fonts
/// are installed and the themes are loaded, again once there is a window — on Linux only a window knows
/// what the desktop's appearance is — and whenever a choice changes.
pub fn apply(window: Option<&mut Window>, cx: &mut App) {
    // Before anything shows a theme: the fonts in force now are the ones
    // the application brought.
    built_in_fonts(cx);

    let mode = match theme_choice() {
        ThemeChoice::Light => ThemeMode::Light,
        ThemeChoice::Dark => ThemeMode::Dark,
        // Android's window is told of a change of appearance but starts out
        // calling itself light whatever the phone says, so the phone is asked.
        #[cfg(target_os = "android")]
        ThemeChoice::System => {
            let _ = &window;
            if gpui_mobile::android::jni::query_night_mode_via_jni() {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            }
        }
        #[cfg(not(target_os = "android"))]
        ThemeChoice::System => window
            .as_ref()
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance())
            .into(),
    };
    // The clock and the icons beside it are drawn over our own title bar, and
    // have to be told which of its colours to stand out against.
    #[cfg(target_os = "android")]
    gpui_mobile::set_system_chrome(&gpui_mobile::SystemChromeStyle {
        status_bar_style: if mode.is_dark() {
            gpui_mobile::StatusBarContentStyle::Light
        } else {
            gpui_mobile::StatusBarContentStyle::Dark
        },
        ..Default::default()
    });
    // Both modes are given what they wear, not only the one showing: the
    // other is what a change of mode will show, here or in the library.
    for each in [ThemeMode::Light, ThemeMode::Dark] {
        match themes::pick(each, theme_name(each).as_deref(), cx) {
            Some(theme) => gpui_adaptive_colors::wear(theme, cx),
            None => gpui_adaptive_colors::wear_scheme(each, cx),
        }
    }
    if Theme::global(cx).mode != mode {
        Theme::change(mode, None, cx);
    }
    // Showing a theme shows it fonts and all, so the fonts go on afterwards.
    apply_fonts(cx);
}

/// Makes the fonts say what the settings say. Whatever shows a theme again
/// — a change of mode, or of the colour a scheme is made from — shows it in
/// the theme's own fonts, and this puts the chosen ones back.
pub fn apply_fonts(cx: &mut App) {
    let built_in = built_in_fonts(cx);
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
