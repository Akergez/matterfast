use serde_json::Value;

/// A colour as `#rrggbb` or `#rrggbbaa`, or nothing for anything else.
pub(crate) fn color(value: &Value) -> Option<String> {
    let hex = value.as_str()?.trim().strip_prefix('#')?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        3 | 4 => Some(format!(
            "#{}",
            hex.chars().flat_map(|digit| [digit, digit]).collect::<String>()
        )),
        6 | 8 => Some(format!("#{hex}")),
        _ => None,
    }
}
