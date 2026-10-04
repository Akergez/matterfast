use serde_json::Value;

use crate::constants::{LENA, ME, MIKK, OLGA, SARA};
use crate::model::user;

/// Everybody whose handle or name begins with `term`, Olga included: she is
/// on the server and in nothing the client has opened.
pub(super) fn directory(term: &str) -> Vec<Value> {
    let term = term.to_lowercase();
    [ME, LENA, MIKK, SARA, OLGA]
        .into_iter()
        .map(user)
        .filter(|u| {
            ["username", "first_name", "last_name"]
                .iter()
                .any(|key| u[key].as_str().unwrap_or_default().to_lowercase().starts_with(&term))
        })
        .collect()
}
