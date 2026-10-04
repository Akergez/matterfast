/// How much of the top and of the bottom of the screen the system draws its
/// own bars over, which the window lies under edge to edge.
#[cfg(target_os = "android")]
pub(super) fn system_bars() -> (f32, f32) {
    gpui_mobile::android::jni::platform()
        .and_then(|platform| platform.primary_window())
        .map(|window| {
            let insets = window.safe_area_insets_logical();
            (insets.top, insets.bottom)
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "android"))]
pub(super) fn system_bars() -> (f32, f32) {
    (0.0, 0.0)
}
