/// How much of the top and of the bottom of the screen the system draws its
/// own bars over, which the window lies under edge to edge. Nothing on a
/// desktop.
pub(super) fn system_bars() -> (f32, f32) {
    let (top, bottom) = gpui_android_glue::insets::system_bars();
    (f32::from(top), f32::from(bottom))
}
