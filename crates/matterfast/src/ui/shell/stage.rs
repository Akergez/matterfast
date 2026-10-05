use std::rc::Rc;

use gpui_kit::Entity;

use super::panes::Panes;
use crate::ui::login::LoginView;
use crate::ui::Ui;

/// What the window is showing.
pub(super) enum Stage {
    /// Waiting on the keyring, or on the first answer from a server.
    Loading,
    Login(Entity<LoginView>),
    /// More than one account is stored, so ask rather than guessing which
    /// one this launch is for. Each is (server, token).
    ChooseServer(Vec<(String, String)>),
    Session(Rc<Ui>, Panes),
}
