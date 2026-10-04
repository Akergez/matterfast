use gpui_kit::{App, Context, Window};

use super::matterfast::Matterfast;
use super::shell_view::Shell;

/// Runs `f` with the window and what is in it, if there is one.
pub(super) fn with_shell(
    cx: &mut App,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    let Some((handle, shell)) = cx.try_global::<Matterfast>().and_then(|app| app.window.clone())
    else {
        return;
    };
    let _ = handle.update(cx, |_, window, cx| {
        shell.update(cx, |shell, cx| f(shell, window, cx));
    });
}
