use gpui_kit::App;

use super::matterfast::Matterfast;
use super::open_window::open_window;
use crate::ipc::Request;

/// Does what a launch asked for — this one, or a later one that found us
/// already running.
pub fn handle_request(request: Request, cx: &mut App) {
    // Raising the application has to *show* something, and a callback from
    // the browser still needs a window to land in.
    present(cx);
    if let Request::Open(uri) = request {
        crate::ui::sso::deliver(&uri, cx);
    }
}

/// Shows the window: the one that is there, or a new one around whatever
/// session is running. An existing window is raised rather than duplicated —
/// a second window would mean a second session and a second websocket.
pub fn present(cx: &mut App) {
    let existing = cx
        .try_global::<Matterfast>()
        .and_then(|app| app.window.as_ref().map(|(handle, _)| *handle));
    match existing {
        Some(handle) if cx.windows().contains(&handle) => {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
        _ => open_window(cx),
    }
}
