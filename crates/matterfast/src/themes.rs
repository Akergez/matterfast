//! Which themes there are to choose from.
//!
//! Two sources, in the order they win a contested name:
//!
//! 1. the toolkit's own light and dark, which is what the window wears until
//!    somebody chooses otherwise — the dark one with its black let up a
//!    little ([`softened`]);
//! 2. the themes directory under the data directory: one subdirectory for
//!    each extension installed from Zed's registry
//!    ([`crate::zed_extensions`]), and any file a person put there by hand.
//!
//! Nobody else's theme is compiled in. What ships is what the toolkit ships;
//! everything more is something the person installed, under its own licence.
//!
//! A file may be in Zed's format or the toolkit's. Zed's files are kept as
//! they were published and translated every time they are read
//! ([`crate::zed_theme`]), so a better translation in a later version reaches
//! the themes already installed.
//!
//! The toolkit has a theme registry that can watch a directory, and it is not
//! used: it reads only its own format, and everything that changes this
//! directory goes through here anyway. The list is read once at startup and
//! again after an install or a removal — a handful of small files, read on
//! the main thread before there is a frame to delay.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::component::{ThemeConfig, ThemeMode, ThemeRegistry, ThemeSet};
use gpui_kit::{App, Global, SharedString};

use crate::zed_extensions::valid_id;
use crate::zed_theme;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Format(#[from] zed_theme::Error),
    #[error("not a theme the toolkit can read: {0}")]
    Shape(#[from] serde_json::Error),
    #[error("{0}")]
    Disk(#[from] std::io::Error),
    #[error("none of the files in this extension is a theme this can read")]
    NothingUsable,
    #[error("\"{0}\" is not an extension name")]
    BadId(String),
}

/// Every theme known right now. The toolkit's own two come first.
struct Catalogue(Vec<Rc<ThemeConfig>>);

impl Global for Catalogue {}

/// Where installed themes are kept.
pub fn dir() -> PathBuf {
    crate::paths::data_dir().join(crate::APP_ID).join("themes")
}

/// The themes in a file of either format.
pub fn read(text: &str) -> Result<Vec<ThemeConfig>, Error> {
    let document = zed_theme::parse(text)?;
    let document = if zed_theme::is_zed(&document) {
        zed_theme::convert(&document)?
    } else {
        document
    };
    Ok(serde_json::from_value::<ThemeSet>(document)?.themes)
}

/// The theme files under `dir`: its own, then those of each subdirectory, in
/// name order so that the same directory always gives the same list.
fn files(dir: &Path) -> Vec<PathBuf> {
    let listed = |dir: &Path| -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        paths.sort();
        paths
    };
    let is_theme = |path: &PathBuf| path.is_file() && path.extension().is_some_and(|e| e == "json");
    let top = listed(dir);
    let mut found: Vec<PathBuf> = top.iter().filter(|path| is_theme(path)).cloned().collect();
    for directory in top.iter().filter(|path| path.is_dir()) {
        found.extend(listed(directory).into_iter().filter(is_theme));
    }
    found
}

/// The themes under `dir`. A file that cannot be read is said so in the log
/// and skipped: one bad file must not cost the others.
fn from_disk(dir: &Path) -> Vec<ThemeConfig> {
    let mut themes = Vec::new();
    for path in files(dir) {
        match std::fs::read_to_string(&path).map_err(Error::from).and_then(|text| read(&text)) {
            Ok(found) => themes.extend(found),
            Err(error) => tracing::warn!(path = %path.display(), %error, "ignored a theme file"),
        }
    }
    themes
}

/// One list out of the sources, the first of a name winning. Names are what
/// the settings store, so there can only be one theme to a name.
fn merged(
    defaults: Vec<Rc<ThemeConfig>>,
    rest: impl IntoIterator<Item = ThemeConfig>,
) -> Vec<Rc<ThemeConfig>> {
    let mut seen: HashSet<SharedString> = defaults.iter().map(|theme| theme.name.clone()).collect();
    let mut themes = defaults;
    for theme in rest {
        if seen.insert(theme.name.clone()) {
            themes.push(Rc::new(theme));
        }
    }
    themes
}

/// What the toolkit's dark theme says, and what is said here instead. Its
/// surfaces are within a hair of pure black and its text within a hair of
/// pure white, which is the most contrast a screen can show and tiring to
/// read a day of chat in. Each step moves one notch along the same grey
/// scale, so the layers keep their order: the page under the bars, the bars
/// under what floats.
///
/// The sidebar starts on the page's own grey and would land on it again, so
/// [`softened`] gives it the title bar's afterwards: the frame is one colour
/// and the page another.
const SOFTER: &[(&str, &str)] = &[
    ("neutral-950", "neutral-900"),
    ("#0a0a0a", "#171717"),
    ("#171717", "#1f1f1f"),
    ("neutral-50", "neutral-200"),
    ("#fafafa", "#e5e5e5"),
];

/// A theme with its colours swapped as [`SOFTER`] says. A colour is matched
/// whole or as the opaque part of a translucent one (`#17171766`), whose
/// alpha is kept. Anything that cannot be rewritten is left as it was: a
/// theme that is a shade too dark is better than none.
fn softened(theme: &ThemeConfig) -> ThemeConfig {
    fn swap(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(color) => {
                let softer = SOFTER.iter().find_map(|(from, to)| {
                    let alpha = color.strip_prefix(from)?;
                    let is_alpha = from.starts_with('#') && alpha.len() == 2;
                    (alpha.is_empty() || is_alpha).then(|| format!("{to}{alpha}"))
                });
                if let Some(softer) = softer {
                    *color = softer;
                }
            }
            serde_json::Value::Object(fields) => fields.values_mut().for_each(swap),
            _ => {}
        }
    }
    let Ok(mut document) = serde_json::to_value(theme) else {
        return theme.clone();
    };
    // Only the colours: a name or a font family is not one, whatever it says.
    if let Some(colors) = document.get_mut("colors") {
        swap(colors);
    }
    let mut soft: ThemeConfig =
        serde_json::from_value(document).unwrap_or_else(|_| theme.clone());
    if soft.colors.title_bar.is_some() {
        soft.colors.sidebar = soft.colors.title_bar.clone();
    }
    soft
}

/// Reads every source again. Call once at startup, before the first theme is
/// applied, and after the themes directory changed.
pub fn load(cx: &mut App) {
    let registry = ThemeRegistry::global(cx);
    let defaults = vec![
        registry.default_light_theme().clone(),
        Rc::new(softened(registry.default_dark_theme())),
    ];
    let themes = merged(defaults, from_disk(&dir()));
    cx.set_global(Catalogue(themes));
}

fn catalogue(cx: &App) -> &[Rc<ThemeConfig>] {
    cx.try_global::<Catalogue>()
        .map(|catalogue| catalogue.0.as_slice())
        .unwrap_or_default()
}

/// The names of the themes of one mode: the toolkit's own first, the rest in
/// alphabetical order.
pub fn names(mode: ThemeMode, cx: &App) -> Vec<SharedString> {
    let mut names: Vec<SharedString> = catalogue(cx)
        .iter()
        .filter(|theme| theme.mode == mode)
        .map(|theme| theme.name.clone())
        .collect();
    if names.len() > 1 {
        names[1..].sort_by_key(|name| name.to_lowercase());
    }
    names
}

/// The theme to wear in `mode`: the one called `wanted` if there is such a
/// theme of that mode, and the toolkit's own otherwise — a theme that was
/// removed since it was chosen must not leave the window without one.
pub fn pick(mode: ThemeMode, wanted: Option<&str>, cx: &App) -> Rc<ThemeConfig> {
    let of_mode = || catalogue(cx).iter().filter(|theme| theme.mode == mode);
    wanted
        .and_then(|wanted| of_mode().find(|theme| theme.name == wanted))
        .or_else(|| of_mode().next())
        .cloned()
        .unwrap_or_else(|| {
            let registry = ThemeRegistry::global(cx);
            if mode.is_dark() {
                registry.default_dark_theme().clone()
            } else {
                registry.default_light_theme().clone()
            }
        })
}

/// The extensions installed from the registry, by their names there.
pub fn installed() -> BTreeSet<String> {
    installed_in(&dir())
}

fn installed_in(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_id(name))
        .collect()
}

/// Keeps the theme files of an extension, replacing what was there under the
/// same name. Answers how many themes that added. Does disk work: call it
/// off the main thread.
pub fn install(id: &str, files: &[String]) -> Result<usize, Error> {
    install_into(&dir(), id, files)
}

fn install_into(dir: &Path, id: &str, files: &[String]) -> Result<usize, Error> {
    if !valid_id(id) {
        return Err(Error::BadId(id.to_string()));
    }
    // Only what can be read is kept, and the count is taken before anything
    // is written: an extension with nothing usable leaves no empty directory
    // behind to be listed as installed.
    let usable: Vec<(&String, usize)> = files
        .iter()
        .filter_map(|file| Some((file, read(file).ok()?.len())))
        .collect();
    if usable.is_empty() {
        return Err(Error::NothingUsable);
    }
    let target = dir.join(id);
    match std::fs::remove_dir_all(&target) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    std::fs::create_dir_all(&target)?;
    for (index, (file, _)) in usable.iter().enumerate() {
        std::fs::write(target.join(format!("{index:02}.json")), file)?;
    }
    Ok(usable.iter().map(|(_, themes)| themes).sum())
}

/// Forgets an installed extension. Does disk work, like [`install`].
pub fn remove(id: &str) -> Result<(), Error> {
    remove_from(&dir(), id)
}

fn remove_from(dir: &Path, id: &str) -> Result<(), Error> {
    if !valid_id(id) {
        return Err(Error::BadId(id.to_string()));
    }
    Ok(std::fs::remove_dir_all(dir.join(id))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATIVE: &str = r##"{"name": "Mine", "themes": [
        {"name": "Mine Dark", "mode": "dark", "colors": {"background": "#101010"}}]}"##;
    const ZED: &str = r##"{"name": "Theirs", "themes": [
        {"name": "Theirs Light", "appearance": "light", "style": {"text": "#202020"}},
        {"name": "Theirs Dark", "appearance": "dark", "style": {"text": "#e0e0e0"}}]}"##;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("matterfast-themes-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_dark_theme_is_softened_one_notch_and_keeps_its_name() {
        let harsh = read(
            r##"{"themes": [{"name": "neutral-950", "mode": "dark", "colors": {
                "background": "neutral-950", "foreground": "neutral-50",
                "sidebar.background": "#0a0a0a", "title_bar.background": "#171717",
                "list.even.background": "#17171766", "border": "neutral-800",
                "caret": "#fafafa", "chart.grid": "neutral-800/60"}}]}"##,
        )
        .unwrap();
        let soft = softened(&harsh[0]);
        // A name that happens to read like a colour is not one.
        assert_eq!(soft.name, "neutral-950");
        let colors = &soft.colors;
        assert_eq!(colors.background.as_deref(), Some("neutral-900"));
        assert_eq!(colors.foreground.as_deref(), Some("neutral-200"));
        // One notch each, not two: what became #171717 does not move again.
        assert_eq!(colors.title_bar.as_deref(), Some("#1f1f1f"));
        // The sidebar is the bar's colour, not the page's.
        assert_eq!(colors.sidebar.as_deref(), Some("#1f1f1f"));
        assert_eq!(colors.list_even.as_deref(), Some("#1f1f1f66"));
        assert_eq!(colors.caret.as_deref(), Some("#e5e5e5"));
        // What is not on the list is not touched.
        assert_eq!(colors.border.as_deref(), Some("neutral-800"));
        assert_eq!(colors.chart_grid.as_deref(), Some("neutral-800/60"));
    }

    #[test]
    fn a_file_in_either_format_is_read() {
        let native = read(NATIVE).unwrap();
        assert_eq!(native[0].name, "Mine Dark");
        assert_eq!(native[0].mode, ThemeMode::Dark);
        let zed = read(ZED).unwrap();
        assert_eq!(zed.len(), 2);
        assert_eq!(zed[0].mode, ThemeMode::Light);
        assert_eq!(zed[0].colors.foreground.as_deref(), Some("#202020"));
    }

    #[test]
    fn what_is_not_a_theme_is_an_error() {
        assert!(matches!(read("nonsense"), Err(Error::Format(_))));
        assert!(matches!(read(r#"{"themes": [{"style": {}}]}"#), Err(Error::Format(_))));
        assert!(matches!(read(r#"{"themes": 4}"#), Err(Error::Shape(_))));
    }

    #[test]
    fn the_first_theme_of_a_name_is_the_one_kept() {
        let named = |name: &str, mode| ThemeConfig { name: name.to_string().into(), mode, ..Default::default() };
        let defaults = vec![Rc::new(named("Default Dark", ThemeMode::Dark))];
        let themes = merged(
            defaults,
            [
                named("A", ThemeMode::Dark),
                named("Default Dark", ThemeMode::Light),
                named("A", ThemeMode::Light),
                named("B", ThemeMode::Light),
            ],
        );
        let listed: Vec<_> = themes.iter().map(|theme| (theme.name.as_ref(), theme.mode)).collect();
        assert_eq!(
            listed,
            [("Default Dark", ThemeMode::Dark), ("A", ThemeMode::Dark), ("B", ThemeMode::Light)],
        );
    }

    #[test]
    fn an_installed_extension_is_listed_read_and_removed() {
        let dir = scratch("install");
        std::fs::write(dir.join("by-hand.json"), NATIVE).unwrap();
        std::fs::write(dir.join("notes.txt"), "not a theme").unwrap();

        let added = install_into(&dir, "theirs", &["broken {".to_string(), ZED.to_string()]).unwrap();
        assert_eq!(added, 2);
        assert_eq!(installed_in(&dir), BTreeSet::from(["theirs".to_string()]));
        let names: Vec<_> = from_disk(&dir).into_iter().map(|theme| theme.name).collect();
        assert_eq!(names, ["Mine Dark", "Theirs Light", "Theirs Dark"]);

        // Installing again replaces, it does not add up.
        install_into(&dir, "theirs", &[ZED.to_string()]).unwrap();
        assert_eq!(from_disk(&dir).len(), 3);

        remove_from(&dir, "theirs").unwrap();
        assert!(installed_in(&dir).is_empty());
        assert_eq!(from_disk(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_extension_with_nothing_usable_leaves_nothing_behind() {
        let dir = scratch("unusable");
        assert!(matches!(
            install_into(&dir, "empty", &["broken {".to_string()]),
            Err(Error::NothingUsable)
        ));
        assert!(installed_in(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_that_is_a_path_is_refused_before_the_disk_is_touched() {
        let dir = scratch("paths");
        assert!(matches!(install_into(&dir, "../out", &[ZED.to_string()]), Err(Error::BadId(_))));
        assert!(matches!(remove_from(&dir, ".."), Err(Error::BadId(_))));
        assert!(dir.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
