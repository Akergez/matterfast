use gpui_kit::component::TitleBar;
use gpui_kit::{px, size, App, AppContext, Bounds, WindowBounds, WindowOptions};

use super::matterfast::Matterfast;
use super::shell_view::Shell;
use super::window_size::initial_size;

pub fn open_window(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            initial_size(),
            cx,
        ))),
        window_min_size: Some(size(px(360.), px(400.))),
        app_id: Some(crate::APP_ID.to_string()),
        ..TitleBar::window_options()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Shell::new(window, cx))
    }) {
        Ok((handle, shell)) => {
            cx.global_mut::<Matterfast>().window = Some((handle, shell.clone()));
            let _ = handle.update(cx, |_, window, cx| {
                window.set_window_title("Matterfast");
                shell.update(cx, |shell, cx| shell.start(window, cx));
            });
            crate::ui::script::play(handle, cx);
        }
        Err(error) => tracing::error!(%error, "could not open a window"),
    }
}
