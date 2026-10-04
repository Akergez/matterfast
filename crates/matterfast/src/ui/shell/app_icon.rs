use std::sync::{Arc, OnceLock};

use gpui_kit::{Image, ImageFormat};

/// The app's own icon — the same file the desktop shows in its launcher.
pub(super) fn app_icon() -> Arc<Image> {
    static ICON: OnceLock<Arc<Image>> = OnceLock::new();
    ICON.get_or_init(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            include_bytes!("../../../../../data/icons/hicolor/scalable/apps/app.akergez.Matterfast.svg")
                .to_vec(),
        ))
    })
    .clone()
}
