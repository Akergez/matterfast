use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::*;

use crate::ui::kit::Lucide;

/// A dock button that shows whether its feature is on.
pub(super) fn toggle(
    id: &'static str,
    icon: Lucide,
    tooltip: &'static str,
    on: bool,
    danger: bool,
) -> Button {
    Button::new(id)
        .icon(Icon::from(icon))
        .small()
        .tooltip(tooltip)
        .when(on && danger, |button| button.danger())
        .when(on && !danger, |button| button.primary())
        .when(!on, |button| button.ghost())
}
