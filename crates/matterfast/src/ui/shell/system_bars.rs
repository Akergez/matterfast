/// How much of the top and of the bottom of the screen the system covers,
/// which the window lies under edge to edge: the status bar above, and below
/// the navigation bar or the on-screen keyboard, whichever covers more. The
/// keyboard is as drawn now, partway while it slides; the window is drawn
/// again on every frame of it. Nothing on a desktop.
pub(super) fn system_bars(window: &gpui_kit::Window) -> (f32, f32) {
    let (top, bars) = gpui_android_glue::insets::system_bars();
    let keyboard = gpui_android_glue::keyboard::keyboard_inset(window);
    (f32::from(top), f32::from(bars.max(keyboard)))
}
