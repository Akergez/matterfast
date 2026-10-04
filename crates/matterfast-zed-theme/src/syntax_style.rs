use serde_json::{Map, Value};

use crate::color::color;

/// How one kind of token is drawn. Zed's `oblique` has no counterpart and is
/// the nearest thing to italic; a weight is whatever number the author typed.
pub(crate) fn syntax_style(style: &Value) -> Option<Value> {
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
