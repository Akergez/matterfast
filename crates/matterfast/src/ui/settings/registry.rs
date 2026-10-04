use crate::zed_extensions::Extension;

/// What is known of the registry's list.
pub(super) enum Registry {
    NotAsked,
    Loading,
    Failed(String),
    Loaded(Vec<Extension>),
}
