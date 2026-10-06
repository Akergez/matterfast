//! What the window can wear, and where it comes from.
//!
//! Two things, and neither is made here:
//!
//! 1. a Material You scheme, worked out from one colour — the system's, or a
//!    calm blue where the system has none to give — which is what the window
//!    wears until somebody chooses otherwise (`gpui-adaptive-colors`);
//! 2. the themes written for the Zed editor that a person installed from its
//!    registry, or put in the themes directory by hand (`gpui-zed-themes`).
//!
//! Nobody else's theme is compiled in. Everything more than the scheme is
//! something the person installed, under its own licence, and is kept as it
//! was published and translated on every read, so a better translation in a
//! later version reaches the themes already installed.
//!
//! What is left for this module is the part a library cannot know: where
//! the themes directory is, which registry to ask, and what a name in the
//! settings means — names are what the settings store, so there can only be
//! one theme to a name, and the scheme has one too.
//!
//! The list is read once at startup and again after an install or a removal
//! — a handful of small files, read on the main thread before there is a
//! frame to delay.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_adaptive_colors::{AdaptiveColors, Color, Options, Seed};
use gpui_kit::component::{ThemeConfig, ThemeMode};
use gpui_kit::{App, Global, SharedString};
use gpui_zed_themes::{Registry, Store};

use crate::background::{set_setting, setting};

/// What the scheme is called in a list of themes, and in the settings: any
/// name that is not a theme's means the scheme, this one included.
pub const SCHEME: &str = "Material You";

/// The colour the scheme is made from where the system has none to give: a
/// desktop without an accent colour, Windows, macOS, an older Android.
const FALLBACK: Color = Color::from_u32(0x3b82f6);

/// The setting the system's last answer is kept under.
const SEED: &str = "system_color";

/// Every installed theme, the first of a name winning.
struct Catalogue(Vec<Rc<ThemeConfig>>);

impl Global for Catalogue {}

/// Where installed themes are kept.
fn dir() -> PathBuf {
    crate::paths::data_dir().join(crate::APP_ID).join("themes")
}

/// The installed themes, as the library keeps them.
pub fn store() -> Store {
    Store::new(dir())
}

/// Zed's registry, unless `MATTERFAST_THEMES_API` says to ask somewhere
/// else: the test server is a stand-in for it.
pub fn registry() -> Registry {
    let registry = Registry::new(concat!("matterfast/", env!("CARGO_PKG_VERSION")));
    match std::env::var("MATTERFAST_THEMES_API") {
        Ok(api) if !api.is_empty() => registry.at(api),
        _ => registry,
    }
}

/// Puts the scheme behind the toolkit's two modes and reads what is
/// installed. Call it once, after the toolkit is set up.
///
/// The system is asked for its colour off the main thread, so its answer
/// comes after the first frame. What it said last time is kept in the
/// settings and handed back here, which is what lets the window open in the
/// right colours rather than change them a moment later.
pub fn init(cx: &mut App) {
    let remembered = setting(SEED)
        .and_then(|value| value.as_str().and_then(|text| text.parse::<Seed>().ok()));
    gpui_adaptive_colors::init(Options::new(FALLBACK).remembered(remembered), cx);

    let mut kept = remembered;
    cx.observe_global::<AdaptiveColors>(move |cx| {
        let said = AdaptiveColors::global(cx).system_seed();
        if said != kept {
            kept = said;
            let text = said.map(|seed| seed.to_string()).unwrap_or_default();
            set_setting(SEED, text.into());
        }
        // The library shows the theme again when the colour changes, and a
        // theme shown again is shown in its own fonts.
        crate::appearance::apply_fonts(cx);
    })
    .detach();

    load(cx);
}

/// One list out of what is on disk, the first of a name winning, and none of
/// them called what the scheme is called.
fn merged(themes: impl IntoIterator<Item = ThemeConfig>) -> Vec<Rc<ThemeConfig>> {
    let mut seen: HashSet<SharedString> = HashSet::from([SCHEME.into()]);
    themes
        .into_iter()
        .filter(|theme| seen.insert(theme.name.clone()))
        .map(Rc::new)
        .collect()
}

/// Reads the themes directory again. Call after it changed.
pub fn load(cx: &mut App) {
    cx.set_global(Catalogue(merged(store().themes())));
}

fn catalogue(cx: &App) -> &[Rc<ThemeConfig>] {
    cx.try_global::<Catalogue>()
        .map(|catalogue| catalogue.0.as_slice())
        .unwrap_or_default()
}

/// The names to choose from in one mode: the scheme first, the installed
/// themes of that mode after it in alphabetical order.
pub fn names(mode: ThemeMode, cx: &App) -> Vec<SharedString> {
    let mut installed: Vec<SharedString> = catalogue(cx)
        .iter()
        .filter(|theme| theme.mode == mode)
        .map(|theme| theme.name.clone())
        .collect();
    installed.sort_by_key(|name| name.to_lowercase());
    let mut names = vec![SharedString::from(SCHEME)];
    names.extend(installed);
    names
}

/// The theme to wear in `mode`: the one called `wanted` if there is such a
/// theme of that mode, and nothing — the scheme — otherwise. A theme that
/// was removed since it was chosen must not leave the window without colours.
///
/// The same theme is the same `Rc` every time it is asked for, until the
/// directory is read again: the library tells a theme already worn by that.
pub fn pick(mode: ThemeMode, wanted: Option<&str>, cx: &App) -> Option<Rc<ThemeConfig>> {
    let wanted = wanted?;
    catalogue(cx)
        .iter()
        .find(|theme| theme.mode == mode && theme.name == wanted)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str, mode: ThemeMode) -> ThemeConfig {
        ThemeConfig { name: name.to_string().into(), mode, ..Default::default() }
    }

    #[test]
    fn the_first_theme_of_a_name_is_the_one_kept() {
        let themes = merged([
            named("A", ThemeMode::Dark),
            named("A", ThemeMode::Light),
            named("B", ThemeMode::Light),
        ]);
        let listed: Vec<_> = themes.iter().map(|theme| (theme.name.as_ref(), theme.mode)).collect();
        assert_eq!(listed, [("A", ThemeMode::Dark), ("B", ThemeMode::Light)]);
    }

    #[test]
    fn a_theme_cannot_take_the_name_of_the_scheme() {
        let themes = merged([named(SCHEME, ThemeMode::Dark), named("A", ThemeMode::Dark)]);
        let listed: Vec<_> = themes.iter().map(|theme| theme.name.as_ref()).collect();
        assert_eq!(listed, ["A"]);
    }
}
