use gpui_kit::{Entity, Subscription};
use gpui_zed_themes::Browser;

use super::names::Names;

pub(super) struct Settings {
    pub(super) light: Entity<Names>,
    pub(super) dark: Entity<Names>,
    pub(super) interface: Entity<Names>,
    pub(super) code: Entity<Names>,
    /// Whether the registry section is open.
    pub(super) browsing: bool,
    /// Zed's registry, to look through and install from.
    pub(super) browser: Entity<Browser>,
    pub(super) _subscriptions: Vec<Subscription>,
}
