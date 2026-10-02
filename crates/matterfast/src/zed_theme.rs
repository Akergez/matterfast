//! Reading a theme written for the Zed editor.
//!
//! Zed's is the theme format people actually publish in — several hundred
//! families in its extension registry — while the toolkit has one of its own
//! that almost nobody writes for. The two agree on how code is highlighted
//! (the toolkit copied that part from Zed on purpose) and on nothing else, so
//! this module is the dictionary between them: a Zed theme family goes in as
//! text, a document in the toolkit's format comes out.
//!
//! It is a translation between two vocabularies that do not line up, and it
//! takes sides where they differ:
//!
//! - Zed is an editor, and its `background` is the frame *around* the editor.
//!   What a chat window is mostly made of is the equivalent of the editor
//!   itself, so `editor.background` is the background here and the panels
//!   become the sidebar. The title bar is the sidebar's colour and not Zed's
//!   own for it: this window has two surfaces, the frame and the page.
//! - Zed has no "primary" colour, the one a default button is filled with.
//!   `text.accent` is the nearest thing a theme author chose on purpose, and
//!   the text on top of it is the background colour, which contrasts with an
//!   accent for the same reason the accent contrasts with the background.
//! - About thirty of the toolkit's hundred and fifty colours are named. The
//!   rest are derived by the toolkit from those, which is also what its own
//!   themes rely on.
//!
//! Everything works on JSON values and nothing here knows about the toolkit's
//! types, so a file somebody wrote by hand can be wrong in any way it likes:
//! a colour that is not a colour is dropped, never passed on to fail later.

use serde_json::{Map, Value, json};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a Zed theme: there is no list of themes in it")]
    NoThemes,
}

/// Where one of the toolkit's colours comes from: the first of these keys of
/// a Zed `style` that holds a colour.
const COLORS: &[(&str, &[&str])] = &[
    ("foreground", &["text", "editor.foreground"]),
    ("muted.foreground", &["text.muted", "text.placeholder"]),
    ("muted.background", &["surface.background", "element.background"]),
    ("border", &["border", "border.variant"]),
    ("input.border", &["border", "border.variant"]),
    ("window.border", &["border", "border.variant"]),
    ("ring", &["border.focused", "text.accent"]),
    ("primary.background", &["text.accent", "link_text.hover"]),
    ("secondary.background", &["element.background"]),
    ("secondary.hover.background", &["element.hover"]),
    ("secondary.active.background", &["element.active", "element.selected"]),
    ("secondary.foreground", &["text"]),
    ("accent.background", &["element.hover", "ghost_element.hover"]),
    ("accent.foreground", &["text"]),
    ("link", &["link_text.hover", "text.accent"]),
    ("link.hover", &["link_text.hover", "text.accent"]),
    ("link.active", &["link_text.hover", "text.accent"]),
    ("popover.foreground", &["text"]),
    ("list.hover.background", &["ghost_element.hover", "element.hover"]),
    ("list.active.background", &["ghost_element.selected", "element.selected"]),
    ("sidebar.foreground", &["text"]),
    ("sidebar.border", &["border.variant", "border"]),
    ("sidebar.accent.background", &["ghost_element.selected", "element.selected"]),
    ("sidebar.accent.foreground", &["text"]),
    ("sidebar.primary.background", &["text.accent"]),
    ("title_bar.border", &["border.variant", "border"]),
    ("tab.foreground", &["text.muted"]),
    ("tab.active.foreground", &["text"]),
    ("scrollbar.background", &["scrollbar.track.background"]),
    ("scrollbar.thumb.background", &["scrollbar.thumb.background"]),
    ("scrollbar.thumb.hover.background", &["scrollbar.thumb.hover_background"]),
    ("drop_target.background", &["drop_target.background"]),
    ("danger.background", &["error"]),
    ("success.background", &["success"]),
    ("warning.background", &["warning"]),
    ("info.background", &["info"]),
    ("base.red", &["terminal.ansi.red", "error"]),
    ("base.green", &["terminal.ansi.green", "success"]),
    ("base.blue", &["terminal.ansi.blue", "info"]),
    ("base.yellow", &["terminal.ansi.yellow", "warning"]),
    ("base.magenta", &["terminal.ansi.magenta"]),
    ("base.cyan", &["terminal.ansi.cyan"]),
];

/// The same, for what the window is filled with. A Zed theme may make these
/// translucent for a blurred window; this window is not one, and a
/// translucent surface on it shows whatever was drawn last underneath, so
/// they lose their alpha.
const SURFACES: &[(&str, &[&str])] = &[
    ("background", &["editor.background", "background"]),
    ("popover.background", &["elevated_surface.background", "surface.background"]),
    ("sidebar.background", &["panel.background", "surface.background"]),
    ("title_bar.background", &["panel.background", "surface.background"]),
    ("status_bar.background", &["status_bar.background"]),
    ("tab_bar.background", &["tab_bar.background"]),
    ("tab.background", &["tab.inactive_background"]),
    ("tab.active.background", &["tab.active_background"]),
];

/// The part of a Zed `style` the toolkit reads as it is, for code blocks.
const HIGHLIGHT: &[&str] = &[
    "editor.background",
    "editor.foreground",
    "editor.active_line.background",
    "editor.line_number",
    "editor.active_line_number",
    "editor.invisible",
    "editor.gutter.background",
    "error",
    "error.background",
    "error.border",
    "warning",
    "warning.background",
    "warning.border",
    "info",
    "info.background",
    "info.border",
    "success",
    "success.background",
    "success.border",
    "hint",
    "hint.background",
    "hint.border",
];

/// Whether a parsed theme file is Zed's rather than the toolkit's: a Zed
/// theme says `appearance` and keeps its colours under `style`.
pub fn is_zed(document: &Value) -> bool {
    document["themes"].as_array().is_some_and(|themes| {
        themes
            .iter()
            .any(|theme| theme.get("style").is_some() || theme.get("appearance").is_some())
    })
}

/// A theme file as text, parsed the way Zed parses one: comments and trailing
/// commas are allowed, and published themes do have them.
pub fn parse(source: &str) -> Result<Value, Error> {
    Ok(serde_json::from_str(&relax(source))?)
}

/// A Zed theme family, as a theme set in the toolkit's format.
pub fn convert(document: &Value) -> Result<Value, Error> {
    let themes = document["themes"].as_array().ok_or(Error::NoThemes)?;
    let themes: Vec<Value> = themes.iter().filter_map(theme).collect();
    if themes.is_empty() {
        return Err(Error::NoThemes);
    }
    let mut set = Map::new();
    set.insert("name".into(), document["name"].as_str().unwrap_or_default().into());
    for key in ["author", "url"] {
        if let Some(text) = document[key].as_str() {
            set.insert(key.into(), text.into());
        }
    }
    set.insert("themes".into(), themes.into());
    Ok(set.into())
}

/// One theme of a family; nothing for one without a name or colours, which
/// could be neither listed nor drawn.
fn theme(theme: &Value) -> Option<Value> {
    let name = theme["name"].as_str().filter(|name| !name.trim().is_empty())?;
    let style = theme["style"].as_object()?;
    // Zed itself treats anything that is not "light" as dark.
    let mode = if theme["appearance"].as_str() == Some("light") { "light" } else { "dark" };

    let first = |keys: &[&str]| keys.iter().find_map(|key| color(style.get(*key)?));
    let mut colors = Map::new();
    for (target, sources) in COLORS {
        if let Some(found) = first(sources) {
            colors.insert((*target).into(), found.into());
        }
    }
    for (target, sources) in SURFACES {
        if let Some(found) = first(sources) {
            colors.insert((*target).into(), opaque(&found).into());
        }
    }
    if let Some(background) = colors.get("background").cloned() {
        colors.insert("primary.foreground".into(), background.clone());
        colors.insert("sidebar.primary.foreground".into(), background);
    }
    // The first "player" is the person at the keyboard: their cursor and
    // their selection are the theme's caret and selection.
    let player = style.get("players").and_then(|players| players.get(0));
    let of_player = |key: &str| player.and_then(|player| color(player.get(key)?));
    if let Some(caret) = of_player("cursor").or_else(|| first(&["text.accent"])) {
        colors.insert("caret".into(), caret.into());
    }
    if let Some(selection) = of_player("selection") {
        colors.insert("selection.background".into(), selection.into());
    }

    let mut highlight = Map::new();
    for key in HIGHLIGHT {
        if let Some(found) = style.get(*key).and_then(color) {
            highlight.insert((*key).into(), found.into());
        }
    }
    // The toolkit will not read a highlight without a `syntax` in it, so a
    // theme that has none gets an empty one rather than being refused whole.
    let syntax: Map<String, Value> = style
        .get("syntax")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(scope, style)| Some((scope.clone(), syntax_style(style)?)))
        .collect();
    highlight.insert("syntax".into(), syntax.into());

    Some(json!({
        "name": name.trim(),
        "mode": mode,
        "colors": colors,
        "highlight": highlight,
    }))
}

/// How one kind of token is drawn. Zed's `oblique` has no counterpart and is
/// the nearest thing to italic; a weight is whatever number the author typed.
fn syntax_style(style: &Value) -> Option<Value> {
    let style = style.as_object()?;
    let mut out = Map::new();
    if let Some(found) = style.get("color").and_then(color) {
        out.insert("color".into(), found.into());
    }
    match style.get("font_style").and_then(Value::as_str) {
        Some("italic" | "oblique") => out.insert("font_style".into(), "italic".into()),
        Some("normal") => out.insert("font_style".into(), "normal".into()),
        _ => None,
    };
    if let Some(weight) = style.get("font_weight").and_then(Value::as_f64) {
        let weight = ((weight / 100.0).round() as i64).clamp(1, 9) * 100;
        out.insert("font_weight".into(), weight.into());
    }
    (!out.is_empty()).then(|| out.into())
}

/// A colour as `#rrggbb` or `#rrggbbaa`, or nothing for anything else.
fn color(value: &Value) -> Option<String> {
    let hex = value.as_str()?.trim().strip_prefix('#')?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        3 | 4 => Some(format!("#{}", hex.chars().flat_map(|digit| [digit, digit]).collect::<String>())),
        6 | 8 => Some(format!("#{hex}")),
        _ => None,
    }
}

/// A colour from [`color`], without its alpha.
fn opaque(color: &str) -> String {
    color.chars().take(7).collect()
}

/// JSON with comments and trailing commas, as plain JSON. What is inside a
/// string is left alone, so a `//` in a URL survives.
fn relax(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                while let Some(c) = chars.next() {
                    out.push(c);
                    match c {
                        '\\' => out.extend(chars.next()),
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                // The newline stays, so an error still names the right line.
                while chars.next_if(|c| *c != '\n').is_some() {}
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for c in chars.by_ref() {
                    if previous == '*' && c == '/' {
                        break;
                    }
                    previous = c;
                }
            }
            '}' | ']' => {
                // A comma with only whitespace between it and the bracket
                // that closes its list.
                let kept = out.trim_end().len();
                if out[..kept].ends_with(',') {
                    out.remove(kept - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A family in Zed's format, written for these tests: the keys are the
    /// ones a published theme has, the colours are nobody's.
    const SAMPLE: &str = r##"{
        "name": "Sample", "author": "Nobody",
        "themes": [
            {"name": "Sample Dark", "appearance": "dark", "style": {
                "background": "#303840ff",
                "editor.background": "#101820ff",
                "panel.background": "#202830ff",
                "title_bar.background": "#404850ff",
                "text": "#e0e4e8ff",
                "text.muted": "#a0a4a8ff",
                "text.accent": "#60a0e0ff",
                "players": [{"cursor": "#60a0e0ff", "selection": "#60a0e03d"}],
                "syntax": {"boolean": {"color": "#c09060ff", "font_style": null,
                                       "font_weight": null}}}},
            {"name": "Sample Light", "appearance": "light", "style": {
                "editor.background": "#fafafaff", "text": "#202020ff"}}
        ]}"##;

    fn converted(source: &str) -> Value {
        convert(&parse(source).unwrap()).unwrap()
    }

    fn named<'a>(set: &'a Value, name: &str) -> &'a Value {
        set["themes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|theme| theme["name"] == name)
            .unwrap()
    }

    #[test]
    fn a_family_becomes_a_theme_set_with_every_theme_in_its_mode() {
        let set = converted(SAMPLE);
        assert_eq!(set["name"], "Sample");
        assert_eq!(set["author"], "Nobody");
        assert_eq!(named(&set, "Sample Dark")["mode"], "dark");
        assert_eq!(named(&set, "Sample Light")["mode"], "light");
    }

    #[test]
    fn the_editor_is_the_background_and_the_panel_is_the_sidebar() {
        let set = converted(SAMPLE);
        let colors = &named(&set, "Sample Dark")["colors"];
        assert_eq!(colors["background"], "#101820");
        assert_eq!(colors["sidebar.background"], "#202830");
        // One frame: the bar over the window is the sidebar's colour.
        assert_eq!(colors["title_bar.background"], colors["sidebar.background"]);
        assert_ne!(colors["title_bar.background"], colors["background"]);
        assert_eq!(colors["foreground"], "#e0e4e8ff");
        assert_eq!(colors["muted.foreground"], "#a0a4a8ff");
    }

    #[test]
    fn the_accent_is_the_primary_colour_and_the_background_is_written_on_it() {
        let set = converted(SAMPLE);
        let colors = &named(&set, "Sample Dark")["colors"];
        assert_eq!(colors["primary.background"], "#60a0e0ff");
        assert_eq!(colors["primary.foreground"], colors["background"]);
    }

    #[test]
    fn the_first_player_gives_the_caret_and_the_selection() {
        let set = converted(SAMPLE);
        let colors = &named(&set, "Sample Dark")["colors"];
        assert_eq!(colors["caret"], "#60a0e0ff");
        assert_eq!(colors["selection.background"], "#60a0e03d");
    }

    #[test]
    fn syntax_colours_come_through_without_the_nulls() {
        let set = converted(SAMPLE);
        let highlight = &named(&set, "Sample Dark")["highlight"];
        assert_eq!(highlight["editor.background"], "#101820ff");
        assert_eq!(highlight["syntax"]["boolean"], json!({ "color": "#c09060ff" }));
    }

    #[test]
    fn a_translucent_surface_loses_its_alpha() {
        let set = converted(
            r##"{"name": "Glass", "themes": [{"name": "Glass", "appearance": "dark",
                "style": {"editor.background": "#10203080", "panel.background": "#abc8"}}]}"##,
        );
        let colors = &named(&set, "Glass")["colors"];
        assert_eq!(colors["background"], "#102030");
        assert_eq!(colors["sidebar.background"], "#aabbcc");
    }

    #[test]
    fn what_is_not_a_colour_is_left_out_rather_than_passed_on() {
        let set = converted(
            r##"{"name": "Odd", "themes": [{"name": "Odd", "appearance": "light", "style": {
                "text": "tomato", "border": null, "text.muted": "#12345",
                "editor.foreground": "#ABCDEF",
                "syntax": {"comment": {"color": 7, "font_style": "oblique", "font_weight": 650},
                           "string": {"color": "nope"}}}}]}"##,
        );
        let theme = named(&set, "Odd");
        // The second source is used when the first holds nonsense.
        assert_eq!(theme["colors"]["foreground"], "#abcdef");
        assert!(theme["colors"].get("border").is_none());
        assert!(theme["colors"].get("muted.foreground").is_none());
        assert_eq!(
            theme["highlight"]["syntax"],
            json!({ "comment": { "font_style": "italic", "font_weight": 700 } }),
        );
    }

    #[test]
    fn an_appearance_that_is_not_light_is_dark() {
        let set = converted(r#"{"themes": [{"name": "A", "style": {}}]}"#);
        assert_eq!(named(&set, "A")["mode"], "dark");
        // And a theme that says nothing about code still has a place for it.
        assert_eq!(named(&set, "A")["highlight"], json!({ "syntax": {} }));
    }

    #[test]
    fn a_theme_without_a_name_or_colours_is_skipped() {
        let set = converted(
            r#"{"themes": [{"name": " ", "style": {}}, {"name": "No style"},
                           {"name": "Kept", "style": {}}]}"#,
        );
        assert_eq!(set["themes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_file_with_no_themes_is_not_a_theme() {
        assert!(matches!(convert(&json!({ "name": "x" })), Err(Error::NoThemes)));
        assert!(matches!(convert(&json!({ "themes": [] })), Err(Error::NoThemes)));
        assert!(matches!(parse("{"), Err(Error::Json(_))));
    }

    #[test]
    fn comments_and_trailing_commas_are_read_past() {
        let document = parse(
            "{\n  // a family\n  \"name\": \"C\", /* inline */\n  \"url\": \"https://a/b//c\",\n  \
             \"note\": \"a \\\" /* not a comment */ , ]\",\n  \"themes\": [1, 2, ],\n}",
        )
        .unwrap();
        assert_eq!(document["url"], "https://a/b//c");
        assert_eq!(document["note"], "a \" /* not a comment */ , ]");
        assert_eq!(document["themes"], json!([1, 2]));
    }

    #[test]
    fn the_two_formats_are_told_apart() {
        assert!(is_zed(&parse(SAMPLE).unwrap()));
        assert!(!is_zed(&json!({ "themes": [{ "name": "N", "mode": "dark", "colors": {} }] })));
        assert!(!is_zed(&json!({ "name": "nothing" })));
    }
}
