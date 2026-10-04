/// Mattermost ids are exactly 26 lowercase alphanumerics, and the client
/// validates that, so pad rather than truncate.
pub(crate) fn id(prefix: &str, n: i64) -> String {
    let mut s = String::from(prefix);
    let tail = format!("{n}");
    while s.len() + tail.len() < 26 {
        s.push('0');
    }
    s.push_str(&tail);
    s.truncate(26);
    s
}
