use gpui_adaptive::{ChoiceOption, ChoiceState};
use gpui_kit::SharedString;

/// A searchable list of names: themes, or font families.
pub(super) type Names = ChoiceState;

/// The options of a list of names, each its own key.
pub(super) fn options(names: impl IntoIterator<Item = SharedString>) -> Vec<ChoiceOption> {
    names.into_iter().map(|name| ChoiceOption::new(name.clone(), name)).collect()
}
