use crate::prepare_with::prepare_with;
use crate::sigil::Sigil;

/// Rewrites a message, linking every mention.
///
/// Everything resolves to itself when nobody says otherwise — the tests have
/// no roster to check against.
pub(crate) fn prepare(message: &str) -> String {
    prepare_with(message, &|handle| Some(handle.to_string()), Sigil::Keep)
}

/// A roster of one.
pub(crate) fn anna(handle: &str) -> Option<String> {
    (handle == "anna").then(|| "Anna Petrova".to_string())
}
