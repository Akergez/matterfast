/// The handle at the start of a candidate, and the name to show for it.
///
/// A handle may contain dots, dashes and underscores, and so may the sentence
/// around it: "ask @anna." names anna, not "anna.". The longest spelling that
/// resolves wins, so a real `first.last` handle is still found whole.
pub(crate) fn resolve_handle<'a>(
    candidate: &'a str,
    known: &dyn Fn(&str) -> Option<String>,
) -> Option<(&'a str, String)> {
    let mut handle = candidate;
    loop {
        if handle.is_empty() {
            return None;
        }
        if let Some(shown) = known(handle) {
            return Some((handle, shown));
        }
        // Only trailing punctuation can be the sentence's rather than the
        // handle's; one mark at a time, so "a.b." tries "a.b" before "a".
        match handle.char_indices().next_back() {
            Some((last, '.' | '-' | '_')) => handle = &handle[..last],
            _ => return None,
        }
    }
}
