use std::path::PathBuf;

use crate::resolve_from::resolve_from;

pub(crate) fn resolve(variable: &str, fallback: &str) -> PathBuf {
    resolve_from(
        std::env::var_os(variable).map(PathBuf::from),
        dirs::home_dir(),
        fallback,
    )
}
