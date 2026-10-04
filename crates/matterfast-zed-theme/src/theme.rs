use serde_json::{json, Map, Value};

use crate::color::color;
use crate::colors::COLORS;
use crate::highlight::HIGHLIGHT;
use crate::opaque::opaque;
use crate::surfaces::SURFACES;
use crate::syntax_style::syntax_style;

/// One theme of a family; nothing for one without a name or colours, which
/// could be neither listed nor drawn.
pub(crate) fn theme(theme: &Value) -> Option<Value> {
    let name = theme["name"].as_str().filter(|name| !name.trim().is_empty())?;
    let style = theme["style"].as_object()?;
    // Zed itself treats anything that is not "light" as dark.
    let mode = if theme["appearance"].as_str() == Some("light") {
        "light"
    } else {
        "dark"
    };

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
