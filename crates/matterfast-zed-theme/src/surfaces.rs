/// The same, for what the window is filled with. A Zed theme may make these
/// translucent for a blurred window; this window is not one, and a
/// translucent surface on it shows whatever was drawn last underneath, so
/// they lose their alpha.
pub(crate) const SURFACES: &[(&str, &[&str])] = &[
    ("background", &["editor.background", "background"]),
    ("popover.background", &["elevated_surface.background", "surface.background"]),
    ("sidebar.background", &["panel.background", "surface.background"]),
    ("title_bar.background", &["panel.background", "surface.background"]),
    ("status_bar.background", &["status_bar.background"]),
    ("tab_bar.background", &["tab_bar.background"]),
    ("tab.background", &["tab.inactive_background"]),
    ("tab.active.background", &["tab.active_background"]),
];
