use serde_json::{Map, Value};

use crate::error::Error;
use crate::theme::theme;

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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_support::{converted, named, SAMPLE};

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
    }
}
