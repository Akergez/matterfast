use serde_json::Value;

use crate::convert::convert;
use crate::parse::parse;

/// A family in Zed's format, written for these tests: the keys are the
/// ones a published theme has, the colours are nobody's.
pub(crate) const SAMPLE: &str = r##"{
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

pub(crate) fn converted(source: &str) -> Value {
    convert(&parse(source).unwrap()).unwrap()
}

pub(crate) fn named<'a>(set: &'a Value, name: &str) -> &'a Value {
    set["themes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|theme| theme["name"] == name)
        .unwrap()
}
