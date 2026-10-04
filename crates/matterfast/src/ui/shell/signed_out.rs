use gpui_kit::App;

use super::matterfast::Matterfast;
use super::with_shell::with_shell;

/// The account was signed out: the session ends, and the window goes back to
/// the sign-in form. Any other server stays signed in, for the next launch.
pub fn signed_out(cx: &mut App) {
    if let Some(ui) = cx.global_mut::<Matterfast>().ui.take() {
        ui.detach(cx);
    }
    cx.global::<Matterfast>().notifier.session(false);
    with_shell(cx, |shell, window, cx| shell.show_login(window, cx));
}
