use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use gpui_kit::component::input::InputState;
use gpui_kit::{Entity, SharedString, Subscription};

use super::names::Names;
use super::registry::Registry;
use crate::ui::Ui;

pub(super) struct Settings {
    pub(super) ui: Rc<Ui>,
    pub(super) light: Entity<Names>,
    pub(super) dark: Entity<Names>,
    pub(super) interface: Entity<Names>,
    pub(super) code: Entity<Names>,
    /// Whether the registry section is open.
    pub(super) browsing: bool,
    pub(super) search: Entity<InputState>,
    pub(super) registry: Registry,
    pub(super) installed: BTreeSet<String>,
    /// Extensions being installed or removed right now.
    pub(super) busy: HashSet<String>,
    /// What the last install or removal had to say.
    pub(super) notice: Option<SharedString>,
    pub(super) _subscriptions: Vec<Subscription>,
}
