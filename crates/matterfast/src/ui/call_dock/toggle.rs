use gpui_adaptive::Command;

use crate::ui::kit::Lucide;

/// A dock button that shows whether its feature is on: filled while it is,
/// in the danger colour when being on is something to notice (recording).
pub(super) fn toggle(
    id: &'static str,
    icon: Lucide,
    tooltip: &'static str,
    on: bool,
    danger: bool,
) -> Command {
    let command = Command::new(id, tooltip).icon(icon);
    match (on, danger) {
        (true, true) => command.destructive(),
        (true, false) => command.primary(),
        (false, _) => command,
    }
}
