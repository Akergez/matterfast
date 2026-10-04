/// A display name as link text: brackets and backslashes are the only
/// characters that could end the label early.
pub(crate) fn escape_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if matches!(ch, '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}
