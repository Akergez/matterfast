use std::rc::Rc;

use gpui_kit::{AnyWindowHandle, App, Entity, Global};

use super::shell_view::Shell;
use crate::notifications::Notifier;
use crate::ui::Ui;

/// What the application holds for as long as it runs.
pub(super) struct Matterfast {
    /// The session, once there is one. It outlives the window.
    pub(super) ui: Option<Rc<Ui>>,
    pub(super) window: Option<(AnyWindowHandle, Entity<Shell>)>,
    pub(super) notifier: Notifier,
}

impl Global for Matterfast {}

/// The session, if somebody is signed in.
pub fn current(cx: &App) -> Option<Rc<Ui>> {
    cx.try_global::<Matterfast>().and_then(|app| app.ui.clone())
}
