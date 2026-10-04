use gpui_kit::SharedString;

/// What the first entry of a font list is called: the font the application
/// starts with, by name, so "the default" is not a mystery.
pub(super) fn built_in_label(family: &SharedString) -> SharedString {
    format!("{family} (default)").into()
}
